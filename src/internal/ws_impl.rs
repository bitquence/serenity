use std::io::Read;

use async_trait::async_trait;
use async_tungstenite::tungstenite::{
    handshake::client::{generate_key, Request},
    Message,
};
use flate2::read::ZlibDecoder;
use futures::{SinkExt, StreamExt};
use tokio::time::timeout;
use tracing::{instrument, warn};
use url::Url;

use crate::gateway::{GatewayError, WsStream};
use crate::internal::prelude::*;
use crate::json::{from_str, to_string};

#[async_trait]
pub trait ReceiverExt {
    async fn recv_json(&mut self) -> Result<Option<Value>>;
}

#[async_trait]
pub trait SenderExt {
    async fn send_json(&mut self, value: &Value) -> Result<()>;
}

#[async_trait]
impl ReceiverExt for WsStream {
    async fn recv_json(&mut self) -> Result<Option<Value>> {
        const TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_millis(500);

        let ws_message = match timeout(TIMEOUT, self.next()).await {
            Ok(Some(Ok(v))) => Some(v),
            Ok(Some(Err(e))) => return Err(e.into()),
            Ok(None) | Err(_) => None,
        };

        convert_ws_message(ws_message)
    }
}

#[async_trait]
impl SenderExt for WsStream {
    async fn send_json(&mut self, value: &Value) -> Result<()> {
        Ok(to_string(value).map(Message::Text).map_err(Error::from).map(|m| self.send(m))?.await?)
    }
}

#[inline]
pub(crate) fn convert_ws_message(message: Option<Message>) -> Result<Option<Value>> {
    const DECOMPRESSION_MULTIPLIER: usize = 3;

    Ok(match message {
        Some(Message::Binary(bytes)) => {
            let mut decompressed = String::with_capacity(bytes.len() * DECOMPRESSION_MULTIPLIER);

            ZlibDecoder::new(&bytes[..]).read_to_string(&mut decompressed).map_err(|why| {
                warn!("Err decompressing bytes: {:?}; bytes: {:?}", why, bytes);

                why
            })?;

            from_str(decompressed.as_mut_str()).map(Some).map_err(|why| {
                warn!("Err deserializing bytes: {:?}; bytes: {:?}", why, bytes);

                why
            })?
        },
        Some(Message::Text(mut payload)) => from_str(&mut payload).map(Some).map_err(|why| {
            warn!("Err deserializing text: {:?}; text: {}", why, payload,);

            why
        })?,
        Some(Message::Close(Some(frame))) => {
            return Err(Error::Gateway(GatewayError::Closed(Some(frame))));
        },
        // Ping/Pong message behaviour is internally handled by tungstenite.
        _ => None,
    })
}

#[instrument]
pub(crate) async fn create_client(url: Url) -> Result<WsStream> {
    let config = async_tungstenite::tungstenite::protocol::WebSocketConfig {
        max_message_size: None,
        max_frame_size: None,
        max_send_queue: None,
        accept_unmasked_frames: false,
    };
    let req = Request::get(url.as_str())
        .header("Host", "gateway.discord.gg")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36",
        )
        .header("Accept", "*/*")
        .header("Accept-Language", "en-US,en;q=0.5")
        .header("Accept-Encoding", "gzip, deflate, br")
        .header("Sec-WebSocket-Version", "13")
        .header("Origin", "https://discord.com")
        .header("Sec-WebSocket-Extensions", "permessage-deflate")
        .header("Sec-WebSocket-Key", generate_key())
        .header("Connection", "keep-alive, Upgrade")
        .header("Sec-Fetch-Dest", "websocket")
        .header("Sec-Fetch-Mode", "websocket")
        .header("Sec-Fetch-Site", "cross-site")
        .header("Pragma", "no-cache")
        .header("Cache-Control", "no-cache")
        .header("Upgrade", "websocket")
        .body(())
        .unwrap();
    let (stream, _) =
        async_tungstenite::tokio::connect_async_with_config(req, Some(config)).await?;

    Ok(stream)
}
