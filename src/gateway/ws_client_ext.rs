use std::time::SystemTime;

use async_trait::async_trait;
use tracing::{debug, instrument, trace};

use crate::client::bridge::gateway::ChunkGuildFilter;
use crate::constants::OpCode;
use crate::gateway::{CurrentPresence, WsStream};
use crate::internal::prelude::*;
use crate::internal::ws_impl::SenderExt;
use crate::json::json;
use crate::model::gateway::GatewayIntents;
use crate::model::id::GuildId;

#[async_trait]
pub trait WebSocketGatewayClientExt {
    async fn send_chunk_guild(
        &mut self,
        guild_id: GuildId,
        shard_info: &[u64; 2],
        limit: Option<u16>,
        filter: ChunkGuildFilter,
        nonce: Option<&str>,
    ) -> Result<()>;

    async fn send_heartbeat(&mut self, shard_info: &[u64; 2], seq: Option<u64>) -> Result<()>;

    async fn send_identify(
        &mut self,
        shard_info: &[u64; 2],
        token: &str,
        intents: GatewayIntents,
    ) -> Result<()>;

    async fn send_presence_update(
        &mut self,
        shard_info: &[u64; 2],
        current_presence: &CurrentPresence,
    ) -> Result<()>;

    async fn send_resume(
        &mut self,
        shard_info: &[u64; 2],
        session_id: &str,
        seq: u64,
        token: &str,
    ) -> Result<()>;
}

#[async_trait]
impl WebSocketGatewayClientExt for WsStream {
    #[instrument(skip(self))]
    async fn send_chunk_guild(
        &mut self,
        guild_id: GuildId,
        shard_info: &[u64; 2],
        limit: Option<u16>,
        filter: ChunkGuildFilter,
        nonce: Option<&str>,
    ) -> Result<()> {
        debug!("[Shard {:?}] Requesting member chunks", shard_info);

        let mut payload = json!({
            "op": OpCode::GetGuildMembers.num(),
            "d": {
                "guild_id": guild_id.as_ref().0.to_string(),
                "limit": limit.unwrap_or(0),
                "nonce": nonce.unwrap_or(""),
            },
        });

        match filter {
            ChunkGuildFilter::None => payload["d"]["query"] = json!(""),
            ChunkGuildFilter::Query(query) => payload["d"]["query"] = json!(query),
            ChunkGuildFilter::UserIds(user_ids) => {
                let ids = user_ids.iter().map(|x| x.0).collect::<Vec<u64>>();
                payload["d"]["user_ids"] = json!(ids);
            },
        };

        self.send_json(&payload).await.map_err(From::from)
    }

    #[instrument(skip(self))]
    async fn send_heartbeat(&mut self, shard_info: &[u64; 2], seq: Option<u64>) -> Result<()> {
        trace!("[Shard {:?}] Sending heartbeat d: {:?}", shard_info, seq);

        self.send_json(&json!({
            "d": seq,
            "op": OpCode::Heartbeat.num(),
        }))
        .await
        .map_err(From::from)
    }

    #[instrument(skip(self, token))]
    async fn send_identify(
        &mut self,
        shard_info: &[u64; 2],
        token: &str,
        _intents: GatewayIntents,
    ) -> Result<()> {
        debug!("[Shard {:?}] Identifying", shard_info);

        self.send_json(&json!({
            "op": OpCode::Identify.num(),
            "d": {
                "token": token,
                "capabilities": 4093,
                "properties": {
                    "os": "Windows",
                    "browser": "Chrome",
                    "device": "",
                    "system_locale": "en-US",
                    "browser_user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/108.0.0.0 Safari/537.36",
                    "browser_version": "108.0.0.0",
                    "os_version": "10",
                    "referrer": "https://discord.com/",
                    "referring_domain": "discord.com",
                    "referrer_current": "",
                    "referring_domain_current": "",
                    "release_channel": "stable",
                    "client_build_number": 166505,
                    "client_event_source": null
                },
                "presence": {
                    "status": "online",
                    "since": 0,
                    "activities": [],
                    "afk": false
                },
                "compress": false,
                "client_state": {
                    "guild_versions": {},
                    "highest_last_message_id": "0",
                    "read_state_version": 0,
                    "user_guild_settings_version": -1,
                    "user_settings_version": -1,
                    "private_channels_version": "0",
                    "api_code_version": 0
                }
            }
        }))
        .await
    }

    #[instrument(skip(self))]
    async fn send_presence_update(
        &mut self,
        shard_info: &[u64; 2],
        current_presence: &CurrentPresence,
    ) -> Result<()> {
        let &(ref activity, ref status) = current_presence;
        let now = SystemTime::now();

        debug!("[Shard {:?}] Sending presence update", shard_info);

        self.send_json(&json!({
            "op": OpCode::StatusUpdate.num(),
            "d": {
                "afk": false,
                "since": now,
                "status": status.name(),
                "game": activity.as_ref().map(|x| json!({
                    "name": x.name,
                    "type": x.kind,
                    "url": x.url,
                })),
            },
        }))
        .await
    }

    #[instrument(skip(self, token))]
    async fn send_resume(
        &mut self,
        shard_info: &[u64; 2],
        session_id: &str,
        seq: u64,
        token: &str,
    ) -> Result<()> {
        debug!("[Shard {:?}] Sending resume; seq: {}", shard_info, seq);

        self.send_json(&json!({
            "op": OpCode::Resume.num(),
            "d": {
                "session_id": session_id,
                "seq": seq,
                "token": token,
            },
        }))
        .await
        .map_err(From::from)
    }
}
