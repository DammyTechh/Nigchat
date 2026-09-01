//! NigChat backend — composition root.
//!
//! This is the only file that knows PostgreSQL, Redis, FCM and APNs exist. It
//! builds the concrete adapters, injects them as trait objects, and starts the
//! HTTP server.
//!
//! Run N of these behind a load balancer. They share PostgreSQL (truth) and
//! Redis (coordination) and hold no local state, so instances can be added,
//! killed or rolled at any moment without a client noticing.

mod config;

use std::sync::Arc;

use anyhow::Context;
use nigchat_api::{build_router, ApiState, RouterConfig};
use nigchat_application::Services;
use nigchat_domain::entities::PushProvider;
use nigchat_domain::ports::PushSender;
use nigchat_infrastructure::push::{ApnsSender, FcmSender, NoopPushSender};
use nigchat_infrastructure::sms::{HttpSmsSender, LoggingSmsSender};
use nigchat_infrastructure::livekit::LiveKitTokens;
use nigchat_infrastructure::storage::SupabaseStorage;
use nigchat_infrastructure::{
    Argon2Hasher, JwtTokenService, PostgresRepositories, RedisEventPublisher, RedisPresence,
    RedisRateLimiter, SystemClock,
};
use tokio::net::TcpListener;
use tokio::signal;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let config = Config::from_env()?;
    init_tracing(config.is_production());

    tracing::info!(
        environment = %config.environment,
        instance = %config.instance_id,
        "starting nigchat-server"
    );

    // --- data stores ------------------------------------------------------
    let repositories =
        PostgresRepositories::connect(&config.database_url, config.database_max_connections)
            .await
            .context("could not connect to PostgreSQL")?;

    repositories
        .migrate()
        .await
        .context("database migration failed")?;
    tracing::info!("migrations applied");

    let redis_client =
        redis::Client::open(config.redis_url.as_str()).context("invalid REDIS_URL")?;
    let redis = redis::aio::ConnectionManager::new(redis_client)
        .await
        .context("could not connect to Redis")?;

    // --- adapters ---------------------------------------------------------
    let sms: Arc<dyn nigchat_domain::ports::SmsSender> =
        match (&config.sms_endpoint, &config.sms_api_key) {
            (Some(endpoint), Some(api_key)) => Arc::new(HttpSmsSender::new(
                endpoint.clone(),
                api_key.clone(),
                config.sms_sender_id.clone(),
            )),
            // Validated above: this branch is unreachable outside development.
            _ => {
                tracing::warn!("no SMS provider configured; codes will not be delivered");
                Arc::new(LoggingSmsSender)
            }
        };

    let push = build_push_senders(&config);

    let storage: Option<Arc<dyn nigchat_domain::ports::ObjectStorage>> =
        match (&config.supabase_url, &config.supabase_service_key) {
            (Some(url), Some(key)) => {
                tracing::info!(bucket = %config.media_bucket, "object storage enabled");
                Some(Arc::new(SupabaseStorage::new(
                    url.clone(),
                    key.clone(),
                    config.media_bucket.clone(),
                )))
            }
            _ => {
                // Not fatal. Messaging works; uploads return a clear error.
                tracing::warn!("object storage not configured; media uploads disabled");
                None
            }
        };

    let media_server: Option<Arc<dyn nigchat_domain::ports::MediaServerTokens>> = match (
        &config.livekit_url,
        &config.livekit_api_key,
        &config.livekit_api_secret,
    ) {
        (Some(url), Some(key), Some(secret)) => {
            tracing::info!(url = %url, "calling enabled");
            Some(Arc::new(LiveKitTokens::new(
                key.clone(),
                secret.clone(),
                url.clone(),
            )))
        }
        _ => {
            tracing::warn!("LiveKit not configured; calling disabled");
            None
        }
    };

    let services = Services {
        users: repositories.users.clone(),
        devices: repositories.devices.clone(),
        sessions: repositories.sessions.clone(),
        challenges: repositories.challenges.clone(),
        keys: repositories.keys.clone(),
        conversations: repositories.conversations.clone(),
        messages: repositories.messages.clone(),
        notifications: repositories.notifications.clone(),
        security: repositories.security.clone(),
        device_links: repositories.device_links.clone(),
        media: repositories.media.clone(),
        calls: repositories.calls.clone(),
        clock: Arc::new(SystemClock),
        rate_limiter: Arc::new(RedisRateLimiter::new(redis.clone())),
        events: Arc::new(RedisEventPublisher::new(
            redis.clone(),
            config.instance_id.clone(),
        )),
        presence: Arc::new(RedisPresence::new(redis.clone())),
        sms,
        hasher: Arc::new(Argon2Hasher::new(config.hash_pepper.clone())),
        storage,
        media_server,
        tokens: Arc::new(JwtTokenService::new(
            &config.jwt_secret,
            config.access_token_ttl_seconds,
        )),
        push,
    };

    // --- HTTP + realtime --------------------------------------------------
    let state = ApiState::new(services, config.otp_debug_echo, config.trust_proxy_headers);

    // Without this subscription the instance would accept sockets and deliver
    // nothing to them.
    nigchat_api::ws::spawn_bus_listener(state.clone(), config.redis_url.clone());

    let app = build_router(
        state,
        RouterConfig {
            allowed_origins: config.cors_allowed_origins.clone(),
            enable_docs: config.enable_docs,
        },
    );

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("could not bind {}", config.bind_addr))?;

    // 0.0.0.0 means "every interface" — it is a bind address, not something a
    // browser can open. Logging it verbatim sends people to
    // http://0.0.0.0:8080 and an ERR_ADDRESS_INVALID, so print a URL that
    // actually works.
    let browsable = format!("http://localhost:{}", config.bind_addr.port());

    tracing::info!(bind = %config.bind_addr, url = %browsable, "listening");
    if config.enable_docs {
        tracing::info!("API documentation at {browsable}/docs");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server error")?;

    Ok(())
}

/// A missing provider is not fatal: dispatch records "no valid token" and
/// messaging continues. Push must never be able to break sending.
fn build_push_senders(config: &Config) -> Vec<Arc<dyn PushSender>> {
    let mut senders: Vec<Arc<dyn PushSender>> = Vec::new();

    match (&config.fcm_project_id, &config.fcm_access_token) {
        (Some(project), Some(token)) => {
            senders.push(Arc::new(FcmSender::new(project.clone(), token.clone())));
            tracing::info!("FCM push enabled");
        }
        _ => {
            tracing::warn!("FCM not configured; Android push disabled");
            senders.push(Arc::new(NoopPushSender::new(PushProvider::Fcm)));
        }
    }

    match (&config.apns_topic, &config.apns_auth_token) {
        (Some(topic), Some(token)) => {
            senders.push(Arc::new(ApnsSender::new(topic.clone(), token.clone())));
            tracing::info!("APNs push enabled");
        }
        _ => {
            tracing::warn!("APNs not configured; iOS push disabled");
            senders.push(Arc::new(NoopPushSender::new(PushProvider::Apns)));
        }
    }

    senders
}

fn init_tracing(json: bool) {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("nigchat_server=info,nigchat_api=info,nigchat_application=info,tower_http=info,sqlx=warn")
    });

    let registry = tracing_subscriber::registry().with(filter);

    if json {
        registry
            .with(tracing_subscriber::fmt::layer().json().flatten_event(true))
            .init();
    } else {
        registry
            .with(tracing_subscriber::fmt::layer().compact())
            .init();
    }
}

/// Drain in-flight requests before exiting, so a rolling deploy never shows a
/// user a failed send.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    tracing::info!("shutdown signal received; draining connections");
}
