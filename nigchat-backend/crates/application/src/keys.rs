//! E2EE key distribution (spec §28).
//!
//! The server is a **key directory**, never a key holder. It stores public
//! identity keys, signed prekeys and one-time prekeys, hands out bundles so
//! two devices can establish a session, and holds nothing that would let it
//! read a message.
//!
//! No cryptography is performed here. Session establishment, ratcheting and
//! encryption all happen on the endpoints, against a reviewed protocol
//! implementation — "never invent cryptography" (Appendix 3).

use nigchat_domain::entities::{DeviceIdentityKey, PreKeyBundle, SecurityEvent, SecurityEventType};
use nigchat_domain::events::{EventEnvelope, ServerEvent};
use nigchat_domain::ids::{DeviceId, UserId};
use nigchat_domain::{DomainError, DomainResult};

use crate::services::Services;

/// Below this, a device is at risk of running out of one-time prekeys and
/// falling back to weaker forward-secrecy properties. It should top up.
pub const PREKEY_LOW_WATER_MARK: i64 = 20;
const MAX_PREKEY_UPLOAD: usize = 200;

pub struct KeyService {
    services: Services,
}

pub struct PublishKeysCommand {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub registration_id: i32,
    pub identity_public_key: Vec<u8>,
    pub signed_prekey_id: i32,
    pub signed_prekey_public: Vec<u8>,
    pub signed_prekey_signature: Vec<u8>,
    pub one_time_prekeys: Vec<(i32, Vec<u8>)>,
}

impl KeyService {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    /// Called once when a device is linked, and again on key rotation.
    ///
    /// A changed identity key on an existing device is a security-relevant
    /// event: peers must be warned, because it is also what a
    /// server-side impersonation attack would look like (spec §28).
    pub async fn publish(&self, command: PublishKeysCommand) -> DomainResult<i32> {
        if command.identity_public_key.is_empty() {
            return Err(DomainError::validation("identity key is required"));
        }
        if command.one_time_prekeys.len() > MAX_PREKEY_UPLOAD {
            return Err(DomainError::validation(
                "too many one-time prekeys in a single upload",
            ));
        }

        let device = self
            .services
            .devices
            .find_by_id(command.device_id)
            .await?
            .ok_or(DomainError::not_found("device"))?;

        if device.user_id != command.user_id {
            return Err(DomainError::Forbidden);
        }

        let key_version = self
            .services
            .keys
            .publish_identity_key(
                command.device_id,
                command.user_id,
                &command.identity_public_key,
                command.registration_id,
            )
            .await?;

        self.services
            .keys
            .publish_signed_prekey(
                command.device_id,
                command.signed_prekey_id,
                &command.signed_prekey_public,
                &command.signed_prekey_signature,
            )
            .await?;

        if !command.one_time_prekeys.is_empty() {
            self.services
                .keys
                .upload_one_time_prekeys(command.device_id, &command.one_time_prekeys)
                .await?;
        }

        // Version 1 is the initial registration and is not a "key change".
        if key_version > 1 {
            self.services
                .security
                .record_event(
                    SecurityEvent::new(command.user_id, SecurityEventType::KeyChanged)
                        .with_device(command.device_id)
                        .with_metadata(serde_json::json!({ "key_version": key_version })),
                )
                .await
                .ok();

            self.notify_peers_of_key_change(command.user_id, command.device_id, key_version)
                .await;
        }

        Ok(key_version)
    }

    /// Hands a sender one bundle per recipient device. The one-time prekey is
    /// consumed — that is the point, and it is why the low-water check below
    /// matters.
    pub async fn bundles_for(&self, target: UserId) -> DomainResult<Vec<PreKeyBundle>> {
        let bundles = self.services.keys.take_prekey_bundles(target).await?;

        if bundles.is_empty() {
            return Err(DomainError::not_found("keys for user"));
        }

        for bundle in &bundles {
            if bundle.one_time_prekey_id.is_none() {
                tracing::warn!(
                    user_id = %target,
                    device_id = %bundle.device_id,
                    "device has exhausted its one-time prekeys"
                );
            }
        }

        Ok(bundles)
    }

    /// The client polls this and uploads more when it is running low.
    pub async fn remaining_prekeys(
        &self,
        user_id: UserId,
        device_id: DeviceId,
    ) -> DomainResult<(i64, bool)> {
        let device = self
            .services
            .devices
            .find_by_id(device_id)
            .await?
            .ok_or(DomainError::not_found("device"))?;

        if device.user_id != user_id {
            return Err(DomainError::Forbidden);
        }

        let count = self.services.keys.one_time_prekey_count(device_id).await?;
        Ok((count, count < PREKEY_LOW_WATER_MARK))
    }

    pub async fn identity_keys(&self, user_id: UserId) -> DomainResult<Vec<DeviceIdentityKey>> {
        self.services.keys.identity_keys_for(user_id).await
    }

    /// Tells everyone in a conversation with this user that the key changed,
    /// so their clients can surface the "security code changed" warning.
    async fn notify_peers_of_key_change(
        &self,
        user_id: UserId,
        device_id: DeviceId,
        key_version: i32,
    ) {
        let conversations = match self.services.conversations.list_for_user(user_id).await {
            Ok(conversations) => conversations,
            Err(err) => {
                tracing::error!(?err, "could not load conversations for key-change fan-out");
                return;
            }
        };

        let mut peers: Vec<UserId> = Vec::new();
        for conversation in conversations {
            if let Ok(members) = self
                .services
                .conversations
                .active_member_ids(conversation.id)
                .await
            {
                peers.extend(members.into_iter().filter(|id| *id != user_id));
            }
        }

        peers.sort_unstable_by_key(|id| id.as_uuid());
        peers.dedup();

        if peers.is_empty() {
            return;
        }

        self.services
            .events
            .publish(EventEnvelope::broadcast(
                peers,
                ServerEvent::KeyChanged {
                    user_id,
                    device_id,
                    key_version,
                },
            ))
            .await
            .ok();
    }
}
