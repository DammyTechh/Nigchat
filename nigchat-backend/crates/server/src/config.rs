//! Configuration.
//!
//! Read once, at boot, from the environment. Nothing else in the codebase
//! touches `std::env`, so every knob is discoverable in one file.
//!
//! Several checks below refuse to start rather than run insecurely. A server
//! that boots with a development secret in production is worse than one that
//! does not boot at all.

use anyhow::{bail, Context, Result};
use std::net::SocketAddr;

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: SocketAddr,
    pub environment: String,
    pub instance_id: String,

    pub database_url: String,
    pub database_max_connections: u32,
    pub redis_url: String,

    pub jwt_secret: String,
    pub hash_pepper: String,
    pub access_token_ttl_seconds: i64,

    pub otp_debug_echo: bool,
    pub trust_proxy_headers: bool,

    pub cors_allowed_origins: Vec<String>,
    pub enable_docs: bool,

    pub sms_endpoint: Option<String>,
    pub sms_api_key: Option<String>,
    pub sms_sender_id: String,

    /// Object storage. Optional — without it the media endpoints refuse
    /// cleanly rather than the whole service failing to start.
    pub supabase_url: Option<String>,
    pub supabase_service_key: Option<String>,
    pub media_bucket: String,

    /// LiveKit — the SFU that carries call audio and video. Optional; without
    /// it the call endpoints refuse cleanly and everything else still runs.
    pub livekit_url: Option<String>,
    pub livekit_api_key: Option<String>,
    pub livekit_api_secret: Option<String>,

    pub fcm_project_id: Option<String>,
    pub fcm_access_token: Option<String>,
    pub apns_topic: Option<String>,
    pub apns_auth_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let environment = optional("ENVIRONMENT").unwrap_or_else(|| "development".to_string());
        let is_development = environment == "development";

        let config = Config {
            bind_addr: optional("BIND_ADDR")
                .unwrap_or_else(|| "0.0.0.0:8080".to_string())
                .parse()
                .context("BIND_ADDR must look like 0.0.0.0:8080")?,
            instance_id: optional("INSTANCE_ID")
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            database_url: required("DATABASE_URL")?,
            database_max_connections: number("DATABASE_MAX_CONNECTIONS", 20) as u32,
            redis_url: required("REDIS_URL")?,
            jwt_secret: required("JWT_SECRET")?,
            hash_pepper: required("HASH_PEPPER")?,
            access_token_ttl_seconds: number("ACCESS_TOKEN_TTL_SECONDS", 900),
            otp_debug_echo: flag("OTP_DEBUG_ECHO", false),
            trust_proxy_headers: flag("TRUST_PROXY_HEADERS", false),
            cors_allowed_origins: optional("CORS_ALLOWED_ORIGINS")
                .map(|value| {
                    value
                        .split(',')
                        .map(str::trim)
                        .filter(|origin| !origin.is_empty())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            enable_docs: flag("ENABLE_DOCS", is_development),
            sms_endpoint: optional("SMS_ENDPOINT"),
            sms_api_key: optional("SMS_API_KEY"),
            sms_sender_id: optional("SMS_SENDER_ID").unwrap_or_else(|| "NigChat".to_string()),
            supabase_url: optional("SUPABASE_URL"),
            supabase_service_key: optional("SUPABASE_SERVICE_KEY"),
            media_bucket: optional("MEDIA_BUCKET").unwrap_or_else(|| "media".to_string()),
            livekit_url: optional("LIVEKIT_URL"),
            livekit_api_key: optional("LIVEKIT_API_KEY"),
            livekit_api_secret: optional("LIVEKIT_API_SECRET"),
            fcm_project_id: optional("FCM_PROJECT_ID"),
            fcm_access_token: optional("FCM_ACCESS_TOKEN"),
            apns_topic: optional("APNS_TOPIC"),
            apns_auth_token: optional("APNS_AUTH_TOKEN"),
            environment,
        };

        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        // 32 bytes minimum. A short HMAC key is brute-forceable, and these two
        // secrets protect every session and every stored hash.
        if self.jwt_secret.len() < 32 {
            bail!("JWT_SECRET must be at least 32 characters");
        }
        if self.hash_pepper.len() < 32 {
            bail!("HASH_PEPPER must be at least 32 characters");
        }
        if self.jwt_secret == self.hash_pepper {
            bail!("JWT_SECRET and HASH_PEPPER must be different values");
        }

        if self.is_development() {
            return Ok(());
        }

        // Returning the OTP in the API response outside development is a
        // full account-takeover vulnerability.
        if self.otp_debug_echo {
            bail!("OTP_DEBUG_ECHO must be false outside development");
        }

        // Without a provider, nobody can ever sign in.
        if self.sms_endpoint.is_none() || self.sms_api_key.is_none() {
            bail!("SMS_ENDPOINT and SMS_API_KEY are required outside development");
        }

        if self.jwt_secret.contains("change_me") || self.hash_pepper.contains("change_me") {
            bail!("refusing to start with the sample secrets from .env.example");
        }

        // Swagger UI describes every endpoint and its auth model. Useful
        // internally, unnecessary exposure on the public internet.
        if self.enable_docs && self.environment == "production" {
            tracing::warn!("API documentation is exposed in production");
        }

        Ok(())
    }

    pub fn is_development(&self) -> bool {
        self.environment == "development"
    }

    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }
}

fn required(key: &str) -> Result<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .with_context(|| format!("missing required environment variable {key}"))
}

fn optional(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

fn number(key: &str, default: i64) -> i64 {
    optional(key)
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn flag(key: &str, default: bool) -> bool {
    optional(key)
        .map(|value| value == "true" || value == "1")
        .unwrap_or(default)
}
