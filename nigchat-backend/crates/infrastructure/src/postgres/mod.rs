//! PostgreSQL adapters.
//!
//! Conventions across every repository here:
//!
//! * **Runtime-checked SQL** (`query_as`) rather than the `query!` macros. The
//!   macros need a live database or committed offline metadata at compile
//!   time, which breaks CI and every new developer's first build. Correctness
//!   is covered by integration tests instead.
//! * **Anything spanning more than one table runs in a transaction.**
//! * Repositories trust their caller. Authorization belongs to the use case;
//!   duplicating it here would let the two copies drift apart.

mod calls;
mod conversations;
mod device_links;
mod identity;
mod media;
mod keys;
mod notifications;

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use nigchat_domain::ports::*;

/// Every repository, built once from one pool.
#[derive(Clone)]
pub struct PostgresRepositories {
    pub pool: PgPool,
    pub users: Arc<dyn UserRepository>,
    pub devices: Arc<dyn DeviceRepository>,
    pub sessions: Arc<dyn SessionRepository>,
    pub challenges: Arc<dyn AuthChallengeRepository>,
    pub keys: Arc<dyn KeyRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub messages: Arc<dyn MessageRepository>,
    pub notifications: Arc<dyn NotificationRepository>,
    pub security: Arc<dyn SecurityRepository>,
    pub device_links: Arc<dyn DeviceLinkRepository>,
    pub media: Arc<dyn MediaRepository>,
    pub calls: Arc<dyn CallRepository>,
}

impl PostgresRepositories {
    pub async fn connect(database_url: &str, max_connections: u32) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            // Fail fast rather than queueing requests behind an exhausted pool.
            .acquire_timeout(std::time::Duration::from_secs(5))
            .connect(database_url)
            .await?;

        Ok(Self::from_pool(pool))
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self {
            users: Arc::new(identity::PgUserRepository::new(pool.clone())),
            devices: Arc::new(identity::PgDeviceRepository::new(pool.clone())),
            sessions: Arc::new(identity::PgSessionRepository::new(pool.clone())),
            challenges: Arc::new(identity::PgChallengeRepository::new(pool.clone())),
            security: Arc::new(identity::PgSecurityRepository::new(pool.clone())),
            device_links: Arc::new(device_links::PgDeviceLinkRepository::new(pool.clone())),
            media: Arc::new(media::PgMediaRepository::new(pool.clone())),
            calls: Arc::new(calls::PgCallRepository::new(pool.clone())),
            keys: Arc::new(keys::PgKeyRepository::new(pool.clone())),
            conversations: Arc::new(conversations::PgConversationRepository::new(pool.clone())),
            messages: Arc::new(conversations::PgMessageRepository::new(pool.clone())),
            notifications: Arc::new(notifications::PgNotificationRepository::new(pool.clone())),
            pool,
        }
    }

    /// Applies the schema. Safe to run from every instance at once: sqlx takes
    /// an advisory lock, so exactly one applies and the rest wait.
    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }

    pub async fn is_healthy(&self) -> bool {
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .is_ok()
    }
}
