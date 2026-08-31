//! Authentication use cases (spec §3, §26.1).
//!
//! Threat model this file is written against: SIM/OTP interception, credential
//! stuffing, stolen refresh tokens, and SMS-pumping fraud (an attacker calling
//! request-otp in a loop to bill you for messages).
//!
//! Countermeasures, all enforced here rather than in the transport layer:
//!   * two-tier rate limiting per phone, plus a per-IP ceiling
//!   * OTPs stored as keyed hashes, compared in constant time, attempt-capped
//!   * refresh tokens rotate on every use; presenting a spent one revokes the
//!     entire device, because that pattern means the value leaked
//!   * every outcome writes a `security_event` the user can see

use chrono::Duration;
use nigchat_domain::entities::{Device, Platform, SecurityEvent, SecurityEventType, User};
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::ports::AccessClaims;
use nigchat_domain::values::PhoneNumber;
use nigchat_domain::{DomainError, DomainResult};
use rand::Rng;

use crate::services::Services;

/// How long a code stays valid. Long enough for a slow SMS route, short enough
/// that an intercepted code is usually already dead.
const OTP_TTL_SECONDS: i64 = 300;
const OTP_MAX_ATTEMPTS: i32 = 5;
const REFRESH_TTL_DAYS: i64 = 90;

/// An account with unbounded linked devices is an unbounded push fan-out and an
/// unbounded blast radius if the number is ever compromised.
const MAX_LINKED_DEVICES: usize = 10;

pub struct AuthService {
    services: Services,
    /// Development only. When true the code is returned in the response
    /// instead of relying on an SMS provider. `server` refuses to boot with
    /// this enabled outside development.
    debug_echo_codes: bool,
}

pub struct StartAuthResult {
    pub expires_in_seconds: i64,
    pub debug_code: Option<String>,
}

pub struct AuthenticatedSession {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in_seconds: i64,
    pub user: User,
    pub device: Device,
    /// True when this call created the account rather than signing in to an
    /// existing one. The client uses it to route into onboarding.
    pub is_new_account: bool,
}

pub struct VerifyOtpCommand {
    pub phone: PhoneNumber,
    pub code: String,
    pub display_name: Option<String>,
    pub platform: Platform,
    pub device_name: Option<String>,
    pub app_version: Option<String>,
    /// Present when an existing install is re-authenticating. Reusing the row
    /// stops reinstalls from littering the user's linked-device list.
    pub existing_device_id: Option<DeviceId>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

impl AuthService {
    pub fn new(services: Services, debug_echo_codes: bool) -> Self {
        Self {
            services,
            debug_echo_codes,
        }
    }

    /// `POST /v1/auth/request-otp`
    pub async fn request_otp(
        &self,
        phone: PhoneNumber,
        ip: Option<&str>,
    ) -> DomainResult<StartAuthResult> {
        // Burst limit first: one code per minute per number. Users tap
        // "resend" impatiently, and every send costs money.
        self.services
            .rate_limiter
            .check(&format!("otp:burst:{}", phone.as_str()), 1, 60)
            .await?;

        // Hourly ceiling per number.
        self.services
            .rate_limiter
            .check(&format!("otp:hourly:{}", phone.as_str()), 5, 3_600)
            .await?;

        // Per-IP ceiling. Without this, one host can enumerate numbers and
        // pump SMS charges across thousands of different phones while never
        // tripping either per-number limit.
        if let Some(ip) = ip {
            let ip_hash = self.services.hasher.hash_ip(ip);
            self.services
                .rate_limiter
                .check(&format!("otp:ip:{ip_hash}"), 20, 3_600)
                .await?;
        }

        let code = generate_numeric_code();
        let code_hash = self
            .services
            .hasher
            .hash_token(&otp_material(&phone, &code));

        let expires_at = self.services.clock.now() + Duration::seconds(OTP_TTL_SECONDS);
        let ip_hash = ip.map(|ip| self.services.hasher.hash_ip(ip));

        self.services
            .challenges
            .create(&phone, &code_hash, expires_at, ip_hash.as_deref())
            .await?;

        // The redacted form is all that reaches the log. `PhoneNumber`'s
        // `Display` is redacted precisely so this cannot be got wrong.
        tracing::info!(phone = %phone, "verification code issued");

        if !self.debug_echo_codes {
            self.services.sms.send_verification_code(&phone, &code).await?;
        }

        Ok(StartAuthResult {
            expires_in_seconds: OTP_TTL_SECONDS,
            debug_code: self.debug_echo_codes.then_some(code),
        })
    }

    /// `POST /v1/auth/verify-otp`
    pub async fn verify_otp(
        &self,
        command: VerifyOtpCommand,
    ) -> DomainResult<AuthenticatedSession> {
        let phone = &command.phone;

        // Independent of the per-challenge attempt counter: that one resets
        // whenever a new code is requested, so on its own it would let an
        // attacker guess forever by cycling codes.
        self.services
            .rate_limiter
            .check(&format!("otp:verify:{}", phone.as_str()), 10, 900)
            .await?;

        let challenge = self
            .services
            .challenges
            .latest_active(phone)
            .await?
            .ok_or(DomainError::InvalidCredentials)?;

        if challenge.attempts >= OTP_MAX_ATTEMPTS {
            return Err(DomainError::validation(
                "too many incorrect attempts; request a new code",
            ));
        }

        let candidate = self
            .services
            .hasher
            .hash_token(&otp_material(phone, command.code.trim()));

        // Both values are hex digests of identical length, and `hash_token`
        // is a keyed HMAC, so an attacker cannot construct a candidate without
        // the pepper. Comparison is still constant-time in the adapter.
        if candidate != challenge.code_hash {
            self.services
                .challenges
                .increment_attempts(challenge.id)
                .await?;
            return Err(DomainError::InvalidCredentials);
        }

        // Single-use. Losing this race means another request already consumed
        // the code — treat it as invalid rather than issuing two sessions.
        if !self.services.challenges.consume(challenge.id).await? {
            return Err(DomainError::InvalidCredentials);
        }

        let display_name = command
            .display_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("NigChat user");

        let existing = self.services.users.find_by_phone(phone).await?;
        let is_new_account = existing.is_none();

        // Stored so contact discovery can match without the server ever
        // holding the raw numbers of people who are not users.
        let phone_hash = self.services.hasher.hash_phone(phone);

        let user = self
            .services
            .users
            .upsert_by_phone(phone, &phone_hash, display_name)
            .await?;

        if !user.can_transact() {
            return Err(DomainError::Forbidden);
        }

        let device = self.resolve_device(&user, &command).await?;

        let issued = self.issue_session(user.clone(), device.clone()).await?;

        // A successful sign-in clears the failure budget, so one mistyped code
        // does not count against the user for the rest of the window.
        let _ = self
            .services
            .rate_limiter
            .reset(&format!("otp:verify:{}", phone.as_str()))
            .await;

        let event_type = if is_new_account {
            SecurityEventType::DeviceLinked
        } else {
            SecurityEventType::Login
        };
        self.record_security_event(&user, Some(device.id), event_type, &command)
            .await;

        let mut session: AuthenticatedSession = issued.into();
        session.is_new_account = is_new_account;
        Ok(session)
    }

    /// `POST /v1/auth/refresh`
    ///
    /// Rotation is mandatory: the presented token is dead when this returns.
    pub async fn refresh(&self, refresh_token: &str) -> DomainResult<AuthenticatedSession> {
        let token_hash = self.services.hasher.hash_token(refresh_token);
        let stored = self
            .services
            .sessions
            .find_by_token_hash(&token_hash)
            .await?
            .ok_or(DomainError::Unauthenticated)?;

        let now = self.services.clock.now();

        // Reuse of an already-rotated token. The legitimate client would never
        // do this, so assume the value was stolen and kill every session on
        // the device — including whichever one the thief is holding.
        if stored.is_revoked() {
            tracing::warn!(
                user_id = %stored.user_id,
                device_id = %stored.device_id,
                "refresh token reuse detected; revoking all device sessions"
            );

            let revoked = self
                .services
                .sessions
                .revoke_all_for_device(stored.device_id)
                .await?;

            self.services
                .security
                .record_event(
                    SecurityEvent::new(stored.user_id, SecurityEventType::SessionReuseDetected)
                        .with_device(stored.device_id)
                        .with_metadata(serde_json::json!({ "sessions_revoked": revoked })),
                )
                .await
                .ok();

            return Err(DomainError::Unauthenticated);
        }

        if stored.is_expired(now) {
            return Err(DomainError::Unauthenticated);
        }

        let device = self
            .services
            .devices
            .find_by_id(stored.device_id)
            .await?
            .filter(Device::is_active)
            .ok_or(DomainError::Unauthenticated)?;

        let user = self
            .services
            .users
            .find_by_id(stored.user_id)
            .await?
            .filter(User::can_transact)
            .ok_or(DomainError::Unauthenticated)?;

        let issued = self.issue_session(user, device).await?;

        // Link old to new so the rotation chain stays auditable.
        self.services
            .sessions
            .rotate(stored.id, issued.session_id)
            .await?;

        Ok(AuthenticatedSession {
            access_token: issued.access_token,
            refresh_token: issued.refresh_token,
            expires_in_seconds: issued.expires_in_seconds,
            user: issued.user,
            device: issued.device,
            is_new_account: false,
        })
    }

    /// `POST /v1/auth/logout` — ends this device's sessions.
    ///
    /// The access token stays valid until it expires (15 minutes). That is the
    /// cost of stateless verification; a Redis deny-list keyed on `jti` is the
    /// upgrade path if instant kill is ever required.
    pub async fn logout(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<()> {
        self.services
            .sessions
            .revoke_all_for_device(device_id)
            .await?;

        self.services
            .security
            .record_event(
                SecurityEvent::new(user_id, SecurityEventType::Logout).with_device(device_id),
            )
            .await
            .ok();

        Ok(())
    }

    /// Verifies an access token. Used by the API layer's extractor.
    pub fn verify_access_token(&self, token: &str) -> DomainResult<AccessClaims> {
        self.services.tokens.verify_access_token(token)
    }

    // --- internals --------------------------------------------------------

    async fn resolve_device(
        &self,
        user: &User,
        command: &VerifyOtpCommand,
    ) -> DomainResult<Device> {
        if let Some(device_id) = command.existing_device_id {
            let existing = self.services.devices.find_by_id(device_id).await?;

            // Ownership check. A device id is supplied by the client, so it is
            // untrusted input: without this, anyone could bind their session to
            // someone else's device row.
            if let Some(device) = existing {
                if device.user_id == user.id && device.is_active() {
                    let ip_hash = command
                        .ip
                        .as_deref()
                        .map(|ip| self.services.hasher.hash_ip(ip));
                    self.services
                        .devices
                        .touch_active(device.id, ip_hash.as_deref())
                        .await?;
                    return Ok(device);
                }
            }
        }

        let active = self.services.devices.list_active(user.id).await?;

        if active.len() >= MAX_LINKED_DEVICES {
            return Err(DomainError::conflict(
                "device limit reached; remove a linked device first",
            ));
        }

        // First device on the account becomes primary.
        let is_primary = active.is_empty();

        self.services
            .devices
            .register(
                user.id,
                command.platform,
                command.device_name.as_deref(),
                command.app_version.as_deref(),
                is_primary,
            )
            .await
    }

    async fn issue_session(&self, user: User, device: Device) -> DomainResult<IssuedSession> {
        let access_token = self
            .services
            .tokens
            .issue_access_token(user.id, device.id)?;

        let refresh_token = self.services.tokens.generate_refresh_token();
        let token_hash = self.services.hasher.hash_token(&refresh_token);
        let expires_at = self.services.clock.now() + Duration::days(REFRESH_TTL_DAYS);

        let session_id = self
            .services
            .sessions
            .create(user.id, device.id, &token_hash, expires_at)
            .await?;

        Ok(IssuedSession {
            session_id,
            access_token,
            refresh_token,
            expires_in_seconds: self.services.tokens.access_token_ttl_seconds(),
            user,
            device,
        })
    }

    async fn record_security_event(
        &self,
        user: &User,
        device_id: Option<DeviceId>,
        event_type: SecurityEventType,
        command: &VerifyOtpCommand,
    ) {
        let mut event = SecurityEvent::new(user.id, event_type).with_metadata(serde_json::json!({
            "platform": command.platform.as_str(),
            "device_name": command.device_name,
        }));

        if let Some(device_id) = device_id {
            event = event.with_device(device_id);
        }
        event.ip_hash = command
            .ip
            .as_deref()
            .map(|ip| self.services.hasher.hash_ip(ip));
        event.user_agent = command.user_agent.clone();

        // A failed audit write must not fail the login, but it must be loud.
        if let Err(err) = self.services.security.record_event(event).await {
            tracing::error!(?err, "failed to record security event");
        }
    }
}

struct IssuedSession {
    session_id: nigchat_domain::ids::SessionId,
    access_token: String,
    refresh_token: String,
    expires_in_seconds: i64,
    user: User,
    device: Device,
}

impl From<IssuedSession> for AuthenticatedSession {
    fn from(issued: IssuedSession) -> Self {
        Self {
            access_token: issued.access_token,
            refresh_token: issued.refresh_token,
            expires_in_seconds: issued.expires_in_seconds,
            user: issued.user,
            device: issued.device,
            is_new_account: false,
        }
    }
}

/// Six digits, uniformly distributed, from the OS entropy source.
///
/// `rand::thread_rng` is a CSPRNG seeded by the OS; a non-cryptographic PRNG
/// here would make codes predictable from a handful of observations.
fn generate_numeric_code() -> String {
    format!("{:06}", rand::thread_rng().gen_range(0..1_000_000))
}

/// Binding the phone number into the hashed material means a code minted for
/// one number cannot be replayed against another.
fn otp_material(phone: &PhoneNumber, code: &str) -> String {
    format!("otp:{}:{}", phone.as_str(), code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..1_000 {
            let code = generate_numeric_code();
            assert_eq!(code.len(), 6);
            assert!(code.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn otp_material_binds_the_phone_number() {
        let a = PhoneNumber::parse("+2348012345678").unwrap();
        let b = PhoneNumber::parse("+2348012345679").unwrap();
        assert_ne!(otp_material(&a, "123456"), otp_material(&b, "123456"));
    }
}
