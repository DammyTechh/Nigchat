//! Redis adapters: rate limiting, cross-instance fan-out, presence.

use async_trait::async_trait;
use nigchat_domain::events::EventEnvelope;
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::ports::{EventPublisher, PresenceRegistry, RateLimiter};
use nigchat_domain::{DomainError, DomainResult};
use redis::AsyncCommands;

use crate::map_redis;

pub const EVENT_CHANNEL: &str = "nigchat.events";

/// Presence keys expire on their own, so a process that dies without cleaning
/// up leaves stale entries for at most this long rather than forever.
const PRESENCE_TTL_SECONDS: i64 = 90;

#[derive(Clone)]
pub struct RedisRateLimiter {
    conn: redis::aio::ConnectionManager,
}

impl RedisRateLimiter {
    pub fn new(conn: redis::aio::ConnectionManager) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl RateLimiter for RedisRateLimiter {
    /// Fixed window. It permits a 2x burst across a window boundary, which is
    /// acceptable for OTP and send limits; a Lua token bucket is the upgrade
    /// path if precision is ever needed.
    async fn check(&self, key: &str, limit: u32, window_seconds: u64) -> DomainResult<()> {
        let mut conn = self.conn.clone();
        let redis_key = format!("rl:{key}");

        let count: u32 = conn.incr(&redis_key, 1u32).await.map_err(map_redis)?;
        if count == 1 {
            let _: () = conn
                .expire(&redis_key, window_seconds as i64)
                .await
                .map_err(map_redis)?;
        }

        if count > limit {
            let ttl: i64 = conn.ttl(&redis_key).await.unwrap_or(window_seconds as i64);
            return Err(DomainError::RateLimited {
                retry_after_seconds: ttl.max(1) as u64,
            });
        }
        Ok(())
    }

    async fn reset(&self, key: &str) -> DomainResult<()> {
        let mut conn = self.conn.clone();
        let _: () = conn.del(format!("rl:{key}")).await.map_err(map_redis)?;
        Ok(())
    }
}

/// Cross-instance fan-out.
///
/// A WebSocket lives on exactly one instance, but the message that must reach
/// it can be written by any instance. Instances therefore never talk to each
/// other — they publish here, and every instance delivers to whichever sockets
/// it owns locally. Swapping this for Redpanda changes this file and nothing
/// else.
#[derive(Clone)]
pub struct RedisEventPublisher {
    conn: redis::aio::ConnectionManager,
    origin: String,
}

impl RedisEventPublisher {
    pub fn new(conn: redis::aio::ConnectionManager, origin: impl Into<String>) -> Self {
        Self {
            conn,
            origin: origin.into(),
        }
    }
}

#[async_trait]
impl EventPublisher for RedisEventPublisher {
    async fn publish(&self, mut envelope: EventEnvelope) -> DomainResult<()> {
        if envelope.recipients.is_empty() {
            return Ok(());
        }
        envelope.origin = Some(self.origin.clone());

        let payload = serde_json::to_string(&envelope)
            .map_err(|_| DomainError::infrastructure("event serialisation failed"))?;

        let mut conn = self.conn.clone();
        redis::cmd("PUBLISH")
            .arg(EVENT_CHANNEL)
            .arg(payload)
            .query_async::<_, ()>(&mut conn)
            .await
            .map_err(map_redis)?;
        Ok(())
    }
}

/// Fleet-wide presence.
///
/// A set per user holding their connected device ids, so "is this user online
/// anywhere?" is one round trip from any instance. The notification policy
/// needs that answer before deciding whether a push is warranted.
#[derive(Clone)]
pub struct RedisPresence {
    conn: redis::aio::ConnectionManager,
}

impl RedisPresence {
    pub fn new(conn: redis::aio::ConnectionManager) -> Self {
        Self { conn }
    }

    fn key(user_id: UserId) -> String {
        format!("presence:{user_id}")
    }
}

#[async_trait]
impl PresenceRegistry for RedisPresence {
    async fn mark_online(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<()> {
        let mut conn = self.conn.clone();
        let key = Self::key(user_id);
        let _: () = conn
            .sadd(&key, device_id.to_string())
            .await
            .map_err(map_redis)?;
        // Refreshed by the socket heartbeat; a crashed instance's entries age
        // out instead of marking a user online forever.
        let _: () = conn
            .expire(&key, PRESENCE_TTL_SECONDS)
            .await
            .map_err(map_redis)?;
        Ok(())
    }

    async fn mark_offline(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<()> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .srem(Self::key(user_id), device_id.to_string())
            .await
            .map_err(map_redis)?;
        Ok(())
    }

    async fn is_online(&self, user_id: UserId) -> DomainResult<bool> {
        let mut conn = self.conn.clone();
        let count: i64 = conn.scard(Self::key(user_id)).await.map_err(map_redis)?;
        Ok(count > 0)
    }

    /// Batched: one pipeline for the whole audience. Per-user calls would turn
    /// a 500-member group fan-out into 500 round trips.
    async fn online_subset(&self, user_ids: &[UserId]) -> DomainResult<Vec<UserId>> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut conn = self.conn.clone();
        let mut pipeline = redis::pipe();
        for user_id in user_ids {
            pipeline.scard(Self::key(*user_id));
        }

        let counts: Vec<i64> = pipeline.query_async(&mut conn).await.map_err(map_redis)?;

        Ok(user_ids
            .iter()
            .zip(counts)
            .filter(|(_, count)| *count > 0)
            .map(|(user_id, _)| *user_id)
            .collect())
    }
}
