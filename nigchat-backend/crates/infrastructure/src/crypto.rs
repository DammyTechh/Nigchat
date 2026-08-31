//! Hashing, token issuing and the clock.

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordVerifier};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::ports::{AccessClaims, Clock, Hasher, TokenService};
use nigchat_domain::values::PhoneNumber;
use nigchat_domain::{DomainError, DomainResult};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Two algorithms, chosen per use:
///
/// * **Argon2id** for anything a human picks (two-step PINs). Slow on purpose.
/// * **HMAC-SHA256 under a server pepper** for high-entropy machine values
///   (refresh tokens, OTPs, phone hashes). Deterministic, so it can be looked
///   up by index, and a slow KDF would buy nothing while costing latency on
///   the hot path.
pub struct Argon2Hasher {
    pepper: String,
}

impl Argon2Hasher {
    pub fn new(pepper: impl Into<String>) -> Self {
        Self {
            pepper: pepper.into(),
        }
    }

    fn hmac(&self, domain: &str, data: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.pepper.as_bytes())
            .expect("HMAC accepts a key of any length");
        // The domain prefix stops a value hashed for one purpose from being
        // replayed as another (an OTP hash matching a token hash, say).
        mac.update(domain.as_bytes());
        mac.update(b":");
        mac.update(data.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}

impl Hasher for Argon2Hasher {
    fn hash_secret(&self, plaintext: &str) -> DomainResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(plaintext.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| DomainError::infrastructure("hashing failed"))
    }

    fn verify_secret(&self, plaintext: &str, hash: &str) -> DomainResult<bool> {
        let parsed =
            PasswordHash::new(hash).map_err(|_| DomainError::infrastructure("bad hash format"))?;
        Ok(Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok())
    }

    fn hash_token(&self, plaintext: &str) -> String {
        self.hmac("token", plaintext)
    }

    fn hash_phone(&self, phone: &PhoneNumber) -> String {
        self.hmac("phone", phone.as_str())
    }

    fn hash_ip(&self, ip: &str) -> String {
        self.hmac("ip", ip)
    }
}

/// Constant-time comparison for digests. Length is not secret, so an early
/// return on a length mismatch is fine.
pub fn secure_compare(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    did: Uuid,
    iat: i64,
    exp: i64,
    jti: Uuid,
}

/// HS256 with a shared secret is correct while this is one service. When the
/// realtime tier is split out (Phase 3), move to EdDSA with a published JWKS
/// so verifiers never hold signing material.
pub struct JwtTokenService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    ttl_seconds: i64,
}

impl JwtTokenService {
    pub fn new(secret: &str, ttl_seconds: i64) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
            ttl_seconds,
        }
    }
}

impl TokenService for JwtTokenService {
    fn issue_access_token(&self, user_id: UserId, device_id: DeviceId) -> DomainResult<String> {
        let now = Utc::now().timestamp();
        let claims = Claims {
            sub: user_id.as_uuid(),
            did: device_id.as_uuid(),
            iat: now,
            exp: now + self.ttl_seconds,
            jti: Uuid::new_v4(),
        };
        encode(&Header::default(), &claims, &self.encoding)
            .map_err(|_| DomainError::infrastructure("token issuing failed"))
    }

    fn verify_access_token(&self, token: &str) -> DomainResult<AccessClaims> {
        let data = decode::<Claims>(token, &self.decoding, &Validation::default())
            .map_err(|_| DomainError::Unauthenticated)?;

        Ok(AccessClaims {
            user_id: UserId::from(data.claims.sub),
            device_id: DeviceId::from(data.claims.did),
            expires_at: data.claims.exp,
        })
    }

    /// 256 bits from the OS entropy source. Opaque and unguessable; its hash
    /// is what reaches the database.
    fn generate_refresh_token(&self) -> String {
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    fn access_token_ttl_seconds(&self) -> i64 {
        self.ttl_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_domains_are_separated() {
        let hasher = Argon2Hasher::new("x".repeat(32));
        let phone = PhoneNumber::parse("+2348012345678").unwrap();
        assert_ne!(hasher.hash_token(phone.as_str()), hasher.hash_phone(&phone));
    }

    #[test]
    fn argon2_round_trip() {
        let hasher = Argon2Hasher::new("x".repeat(32));
        let hash = hasher.hash_secret("123456").unwrap();
        assert!(hasher.verify_secret("123456", &hash).unwrap());
        assert!(!hasher.verify_secret("654321", &hash).unwrap());
    }

    #[test]
    fn jwt_round_trip() {
        let service = JwtTokenService::new(&"s".repeat(32), 900);
        let user = UserId::new();
        let device = DeviceId::new();
        let token = service.issue_access_token(user, device).unwrap();
        let claims = service.verify_access_token(&token).unwrap();
        assert_eq!(claims.user_id, user);
        assert_eq!(claims.device_id, device);
        assert!(service.verify_access_token("garbage").is_err());
    }
}
