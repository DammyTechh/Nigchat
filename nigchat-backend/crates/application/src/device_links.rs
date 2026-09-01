//! QR device linking (spec §11).
//!
//! The browser has no password to type — by design, because a password on a web
//! form is phishable and a stolen one hands over the whole account. Instead the
//! phone, which already holds a verified session, authorises the browser.
//!
//! ```text
//!   browser  POST /v1/devices/link-requests   -> { code, expires_in }
//!            renders the code as a QR, then polls
//!   phone    scans it
//!            POST /v1/devices/link-requests/{code}/approve   (authenticated)
//!   browser  GET  /v1/devices/link-requests/{code}  -> token pair, once
//! ```
//!
//! Properties that matter:
//!
//! * The code lives 60 seconds. A pairing QR left on an unattended screen is an
//!   account takeover waiting to happen.
//! * Only a hash is stored, so a database leak cannot be used to claim a
//!   pending link.
//! * Approval is a single conditional write, so two phones cannot both approve
//!   the same code.
//! * The token pair is handed to the browser exactly once and the row is then
//!   deleted; a replayed poll gets nothing.
//! * Approving is rate limited per user — a phone that approves twenty browsers
//!   in a minute is not a person.

use chrono::Duration;
use nigchat_domain::entities::{Platform, SecurityEvent, SecurityEventType};
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::{DomainError, DomainResult};
use rand::RngCore;

use crate::services::Services;

/// Short on purpose. Long enough to pick up a phone, short enough that a code
/// left on screen is useless by the time anyone else sees it.
const LINK_TTL_SECONDS: i64 = 60;

pub struct DeviceLinkService {
    services: Services,
}

pub struct LinkRequest {
    /// Rendered into the QR. Opaque and single-use.
    pub code: String,
    pub expires_in_seconds: i64,
}

pub enum LinkStatus {
    /// Still waiting for a phone to scan.
    Pending,
    /// Approved — the token pair is returned once and never again.
    Approved {
        user_id: UserId,
        device_id: DeviceId,
        access_token: String,
        refresh_token: String,
        expires_in_seconds: i64,
    },
    /// Expired, already claimed, or never existed. Deliberately one variant:
    /// distinguishing them would let someone probe for live codes.
    Gone,
}

impl DeviceLinkService {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    /// Called by the browser. Unauthenticated — there is no session yet.
    pub async fn request(
        &self,
        platform: Platform,
        device_name: Option<&str>,
        ip: Option<&str>,
    ) -> DomainResult<LinkRequest> {
        // Unauthenticated endpoint, so the only handle for a limit is the
        // caller's address. Without it this is a free way to fill the table.
        if let Some(ip) = ip {
            let ip_hash = self.services.hasher.hash_ip(ip);
            self.services
                .rate_limiter
                .check(&format!("link:request:{ip_hash}"), 30, 3_600)
                .await?;
        }

        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let code = hex::encode(bytes);

        let code_hash = self.services.hasher.hash_token(&code);
        let expires_at = self.services.clock.now() + Duration::seconds(LINK_TTL_SECONDS);

        self.services
            .device_links
            .create(&code_hash, platform.as_str(), device_name, expires_at)
            .await?;

        Ok(LinkRequest {
            code,
            expires_in_seconds: LINK_TTL_SECONDS,
        })
    }

    /// Called by the phone after scanning. The caller's own session is what
    /// authorises the new device — this is the whole trust model.
    pub async fn approve(&self, user_id: UserId, code: &str) -> DomainResult<DeviceId> {
        self.services
            .rate_limiter
            .check(&format!("link:approve:{user_id}"), 10, 3_600)
            .await?;

        let code_hash = self.services.hasher.hash_token(code);

        let pending = self
            .services
            .device_links
            .find_pending(&code_hash)
            .await?
            .ok_or_else(|| DomainError::validation("this code has expired or was already used"))?;

        if pending.approved {
            return Err(DomainError::validation("this code was already used"));
        }

        let platform = Platform::parse(&pending.platform)?;

        // A real device row, so the browser appears in the user's linked
        // devices list and can be signed out from the phone like any other.
        let device = self
            .services
            .devices
            .register(
                user_id,
                platform,
                pending.device_name.as_deref(),
                None,
                false,
            )
            .await?;

        // Conditional write: whoever lands it wins, and a second phone
        // approving the same code gets `false`.
        let claimed = self
            .services
            .device_links
            .approve(&code_hash, user_id, device.id)
            .await?;

        if !claimed {
            self.services
                .devices
                .revoke(device.id, "link_race_lost")
                .await
                .ok();
            return Err(DomainError::conflict("this code was already used"));
        }

        // A new device on the account is exactly the event a user needs to see
        // if it was not them.
        self.services
            .security
            .record_event(
                SecurityEvent::new(user_id, SecurityEventType::DeviceLinked)
                    .with_device(device.id)
                    .with_metadata(serde_json::json!({
                        "platform": pending.platform,
                        "via": "qr_link",
                    })),
            )
            .await
            .ok();

        Ok(device.id)
    }

    /// Polled by the browser. Returns the token pair once, then the row is gone.
    pub async fn poll(&self, code: &str) -> DomainResult<LinkStatus> {
        let code_hash = self.services.hasher.hash_token(code);

        let Some(pending) = self.services.device_links.find_pending(&code_hash).await? else {
            return Ok(LinkStatus::Gone);
        };

        if pending.expires_at <= self.services.clock.now() {
            return Ok(LinkStatus::Gone);
        }
        if !pending.approved {
            return Ok(LinkStatus::Pending);
        }

        let Some(approved) = self.services.device_links.consume(&code_hash).await? else {
            // Another tab polled first and took it.
            return Ok(LinkStatus::Gone);
        };

        let access_token = self
            .services
            .tokens
            .issue_access_token(approved.user_id, approved.device_id)?;

        let refresh_token = self.services.tokens.generate_refresh_token();
        let token_hash = self.services.hasher.hash_token(&refresh_token);
        let expires_at = self.services.clock.now() + Duration::days(90);

        self.services
            .sessions
            .create(approved.user_id, approved.device_id, &token_hash, expires_at)
            .await?;

        Ok(LinkStatus::Approved {
            user_id: approved.user_id,
            device_id: approved.device_id,
            access_token,
            refresh_token,
            expires_in_seconds: self.services.tokens.access_token_ttl_seconds(),
        })
    }
}
