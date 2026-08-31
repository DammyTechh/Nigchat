//! Messaging use cases (spec §4, §26.3).
//!
//! The order of operations in `send` is the important part of this file:
//!
//!   authorize → validate → persist (one transaction) → fan out → notify
//!
//! Persistence commits before anything is published. If fan-out then fails,
//! the message is still durable and the recipient picks it up on next sync —
//! so a realtime hiccup can never lose a message. The reverse order would
//! occasionally deliver a message that was never stored.

use nigchat_domain::entities::{
    Conversation, ConversationKind, Message, MessageKind, NewMessage,
};
use nigchat_domain::events::{EventEnvelope, ServerEvent, TypingState};
use nigchat_domain::ids::{ClientMessageId, ConversationId, DeviceId, MediaId, MessageId, UserId};
use nigchat_domain::values::{Cursor, Seq};
use nigchat_domain::{DomainError, DomainResult};

use crate::notifications::{NotificationDispatcher, NotifyMessageCommand};
use crate::services::Services;

/// Ciphertext ceiling. Large media never travels through this service — it
/// goes to object storage — so anything above this is a bug or an attack.
const MAX_CIPHERTEXT_BYTES: usize = 128 * 1024;
const MAX_MENTIONS: usize = 128;

pub struct MessagingService {
    services: Services,
    dispatcher: NotificationDispatcher,
}

pub struct SendMessageCommand {
    pub conversation_id: ConversationId,
    pub sender_id: UserId,
    pub sender_device_id: DeviceId,
    pub client_message_id: ClientMessageId,
    pub kind: MessageKind,
    pub ciphertext: Vec<u8>,
    pub envelope_version: i16,
    pub metadata: serde_json::Value,
    pub reply_to_id: Option<MessageId>,
    pub mentions: Vec<UserId>,
    pub media_ids: Vec<MediaId>,
}

pub struct SendMessageResult {
    pub message: Message,
    /// False when this was an idempotent replay. The API returns 200 rather
    /// than 201, and no notification is sent a second time.
    pub created: bool,
}

impl MessagingService {
    pub fn new(services: Services) -> Self {
        let dispatcher = NotificationDispatcher::new(services.clone());
        Self {
            services,
            dispatcher,
        }
    }

    pub async fn send(&self, command: SendMessageCommand) -> DomainResult<SendMessageResult> {
        let conversation = self.load_conversation(command.conversation_id).await?;

        // --- authorize ----------------------------------------------------
        let membership = self
            .services
            .conversations
            .membership(command.conversation_id, command.sender_id)
            .await?
            .filter(|member| member.is_active())
            // Same error whether the conversation is missing or the caller is
            // simply not a member: probing must not reveal that it exists.
            .ok_or(DomainError::Forbidden)?;

        if !conversation.can_post(membership.role) {
            return Err(DomainError::Forbidden);
        }

        if !command.kind.is_client_authorable() {
            return Err(DomainError::Forbidden);
        }

        // In a direct chat, a block stops delivery in both directions. Checked
        // before the write so blocked content never enters the database.
        if conversation.kind == ConversationKind::Direct {
            self.assert_not_blocked(&conversation, command.sender_id)
                .await?;
        }

        // --- validate -----------------------------------------------------
        if command.ciphertext.is_empty() {
            return Err(DomainError::validation("message ciphertext is required"));
        }
        if command.ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(DomainError::validation(
                "message exceeds the maximum encrypted size",
            ));
        }
        if command.mentions.len() > MAX_MENTIONS {
            return Err(DomainError::validation("too many mentions"));
        }

        // Resolved here so the notification policy can treat a reply as more
        // specific than a plain message.
        let mut reply_to_author: Option<UserId> = None;
        if let Some(reply_to) = command.reply_to_id {
            let parent = self
                .services
                .messages
                .find_by_id(reply_to)
                .await?
                .filter(|message| message.conversation_id == command.conversation_id)
                .ok_or_else(|| {
                    DomainError::validation("reply target is not a message in this conversation")
                })?;
            reply_to_author = parent.sender_id;
        }

        // Mentions are client-supplied. Without this filter a sender could name
        // any user id at all, and a non-member would be stored as mentioned —
        // noise at best, and a way to probe membership at worst.
        let members = self
            .services
            .conversations
            .active_member_ids(command.conversation_id)
            .await?;
        let mentions: Vec<UserId> = command
            .mentions
            .iter()
            .copied()
            .filter(|id| members.contains(id))
            .collect();

        // Abuse ceiling, not a product limit.
        self.services
            .rate_limiter
            .check(&format!("msg:send:{}", command.sender_id), 60, 10)
            .await?;

        // --- persist ------------------------------------------------------
        let expires_at = conversation
            .disappearing_seconds
            .map(|seconds| self.services.clock.now() + chrono::Duration::seconds(seconds as i64));

        let (message, created) = self
            .services
            .messages
            .append(NewMessage {
                conversation_id: command.conversation_id,
                sender_id: command.sender_id,
                sender_device_id: command.sender_device_id,
                client_message_id: command.client_message_id,
                kind: command.kind,
                ciphertext: command.ciphertext,
                envelope_version: command.envelope_version,
                metadata: command.metadata,
                reply_to_id: command.reply_to_id,
                mentions: mentions.clone(),
                media_ids: command.media_ids,
                expires_at,
            })
            .await?;

        // A retried send from a flaky network. The client gets its original
        // message back; nobody is notified twice.
        if !created {
            tracing::debug!(
                conversation_id = %command.conversation_id,
                "idempotent replay of client_message_id"
            );
            return Ok(SendMessageResult { message, created });
        }

        // The sender has by definition read their own message.
        self.services
            .conversations
            .advance_read_marker(command.conversation_id, command.sender_id, message.seq)
            .await
            .ok();

        // --- fan out ------------------------------------------------------
        let recipients = members;

        let envelope = EventEnvelope::broadcast(
            recipients.clone(),
            ServerEvent::MessageCreated {
                conversation_id: message.conversation_id,
                message_id: message.id,
                seq: message.seq,
                sender_id: message.sender_id,
                kind: message.kind.as_str().to_string(),
                ciphertext: message.ciphertext.clone(),
                created_at: message.created_at,
            },
        );

        // Never fail the request on a publish error: the message is committed
        // and sync will heal the gap.
        if let Err(err) = self.services.events.publish(envelope).await {
            tracing::error!(
                ?err,
                conversation_id = %command.conversation_id,
                seq = %message.seq,
                "message committed but realtime publish failed"
            );
        }

        // --- notify -------------------------------------------------------
        self.dispatcher
            .notify_new_message(NotifyMessageCommand {
                conversation: conversation.clone(),
                message: message.clone(),
                recipients,
                mentions,
                reply_to_author,
            })
            .await;

        Ok(SendMessageResult { message, created })
    }

    /// Keyset pagination. `after_seq` catches a device up; `before_seq`
    /// scrolls into history.
    pub async fn page(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        cursor: Cursor,
    ) -> DomainResult<(Vec<Message>, bool)> {
        self.assert_member(conversation_id, user_id).await?;
        self.services.messages.page(conversation_id, cursor).await
    }

    pub async fn edit(
        &self,
        message_id: MessageId,
        editor: UserId,
        ciphertext: Vec<u8>,
    ) -> DomainResult<Message> {
        if ciphertext.is_empty() || ciphertext.len() > MAX_CIPHERTEXT_BYTES {
            return Err(DomainError::validation("invalid edited message body"));
        }

        let existing = self
            .services
            .messages
            .find_by_id(message_id)
            .await?
            .ok_or(DomainError::not_found("message"))?;

        // Author-only, text-only, inside the edit window. The rule lives on
        // the entity so it cannot drift between call sites.
        if !existing.can_be_edited_by(editor, self.services.clock.now()) {
            return Err(DomainError::Forbidden);
        }

        let updated = self
            .services
            .messages
            .edit(message_id, editor, &ciphertext)
            .await?;

        let recipients = self
            .services
            .conversations
            .active_member_ids(updated.conversation_id)
            .await?;

        self.services
            .events
            .publish(EventEnvelope::broadcast(
                recipients,
                ServerEvent::MessageEdited {
                    conversation_id: updated.conversation_id,
                    seq: updated.seq,
                    ciphertext: updated.ciphertext.clone(),
                    edited_at: updated.edited_at.unwrap_or_else(|| self.services.clock.now()),
                },
            ))
            .await
            .ok();

        Ok(updated)
    }

    /// Soft delete. The row and its `seq` survive so other devices learn the
    /// message is gone instead of finding a hole in the sequence.
    ///
    /// `for_everyone` requires authorship or admin rights; anyone may delete
    /// for themselves.
    pub async fn delete(
        &self,
        message_id: MessageId,
        actor: UserId,
        for_everyone: bool,
    ) -> DomainResult<Seq> {
        let message = self
            .services
            .messages
            .find_by_id(message_id)
            .await?
            .ok_or(DomainError::not_found("message"))?;

        let membership = self
            .services
            .conversations
            .membership(message.conversation_id, actor)
            .await?
            .filter(|member| member.is_active())
            .ok_or(DomainError::Forbidden)?;

        if for_everyone {
            let is_author = message.sender_id == Some(actor);
            if !is_author && !membership.role.can_administer() {
                return Err(DomainError::Forbidden);
            }
        }

        let seq = self
            .services
            .messages
            .soft_delete(message_id, actor, for_everyone)
            .await?;

        if for_everyone {
            let recipients = self
                .services
                .conversations
                .active_member_ids(message.conversation_id)
                .await?;

            self.services
                .events
                .publish(EventEnvelope::broadcast(
                    recipients,
                    ServerEvent::MessageDeleted {
                        conversation_id: message.conversation_id,
                        seq,
                        for_everyone,
                    },
                ))
                .await
                .ok();
        }

        Ok(seq)
    }

    /// Read receipts are high-water marks, so one call covers every message up
    /// to `seq` and a 500-member group emits one event instead of 500.
    pub async fn mark_read(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        seq: Seq,
    ) -> DomainResult<Seq> {
        self.assert_member(conversation_id, user_id).await?;

        let marker = self
            .services
            .conversations
            .advance_read_marker(conversation_id, user_id, seq)
            .await?;

        // Respect the reader's privacy setting: if they have read receipts
        // switched off, the marker is stored (they still need it for their own
        // unread counts) but never broadcast.
        let privacy = self.services.users.privacy_settings(user_id).await?;
        if privacy.read_receipts_enabled {
            let recipients = self
                .services
                .conversations
                .active_member_ids(conversation_id)
                .await?;

            self.services
                .events
                .publish(
                    EventEnvelope::broadcast(
                        recipients,
                        ServerEvent::ReadReceipt {
                            conversation_id,
                            user_id,
                            last_read_seq: marker,
                        },
                    )
                    .excluding(user_id),
                )
                .await
                .ok();
        }

        Ok(marker)
    }

    /// Delivery receipt — the "delivered" tick.
    ///
    /// Separate from `mark_read` because a device can receive a message long
    /// before the user opens the conversation, and the two ticks mean
    /// different things to the sender.
    pub async fn mark_delivered(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        seq: Seq,
    ) -> DomainResult<Seq> {
        self.assert_member(conversation_id, user_id).await?;

        let marker = self
            .services
            .conversations
            .advance_delivery_marker(conversation_id, user_id, seq)
            .await?;

        let recipients = self
            .services
            .conversations
            .active_member_ids(conversation_id)
            .await?;

        self.services
            .events
            .publish(
                EventEnvelope::broadcast(
                    recipients,
                    ServerEvent::DeliveryReceipt {
                        conversation_id,
                        user_id,
                        last_delivered_seq: marker,
                    },
                )
                .excluding(user_id),
            )
            .await
            .ok();

        Ok(marker)
    }

    /// Typing indicators are ephemeral: published, never stored. A dropped one
    /// costs nothing, so this path must never touch the database.
    pub async fn typing(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        state: TypingState,
    ) -> DomainResult<()> {
        let privacy = self.services.users.privacy_settings(user_id).await?;
        if !privacy.typing_indicators_enabled {
            return Ok(());
        }

        let recipients = self
            .services
            .conversations
            .active_member_ids(conversation_id)
            .await?;

        if !recipients.contains(&user_id) {
            // Not a member. Drop silently rather than leaking existence.
            return Ok(());
        }

        self.services
            .events
            .publish(
                EventEnvelope::broadcast(
                    recipients,
                    ServerEvent::Typing {
                        conversation_id,
                        user_id,
                        state,
                    },
                )
                .excluding(user_id),
            )
            .await
    }

    pub async fn set_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
        removed: bool,
    ) -> DomainResult<()> {
        // Emoji arrive from clients on eight platforms; cap the length rather
        // than trying to validate against a Unicode table that will be stale
        // by the next release.
        if emoji.chars().count() > 8 {
            return Err(DomainError::validation("invalid reaction"));
        }

        let message = self
            .services
            .messages
            .find_by_id(message_id)
            .await?
            .ok_or(DomainError::not_found("message"))?;

        self.assert_member(message.conversation_id, user_id).await?;

        self.services
            .messages
            .set_reaction(message_id, user_id, emoji, removed)
            .await?;

        let recipients = self
            .services
            .conversations
            .active_member_ids(message.conversation_id)
            .await?;

        self.services
            .events
            .publish(EventEnvelope::broadcast(
                recipients,
                ServerEvent::ReactionChanged {
                    conversation_id: message.conversation_id,
                    message_id,
                    user_id,
                    emoji: emoji.to_string(),
                    removed,
                },
            ))
            .await
            .ok();

        Ok(())
    }

    // --- internals --------------------------------------------------------

    async fn load_conversation(&self, id: ConversationId) -> DomainResult<Conversation> {
        self.services
            .conversations
            .find_by_id(id)
            .await?
            .ok_or(DomainError::Forbidden)
    }

    async fn assert_member(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<()> {
        self.services
            .conversations
            .membership(conversation_id, user_id)
            .await?
            .filter(|member| member.is_active())
            .map(|_| ())
            .ok_or(DomainError::Forbidden)
    }

    /// A block is symmetric for delivery: neither party can reach the other.
    async fn assert_not_blocked(
        &self,
        conversation: &Conversation,
        sender_id: UserId,
    ) -> DomainResult<()> {
        let members = self
            .services
            .conversations
            .active_member_ids(conversation.id)
            .await?;

        let peer = members.into_iter().find(|id| *id != sender_id);
        let Some(peer) = peer else {
            return Ok(());
        };

        let blocked_by_peer = self.services.users.is_blocked(peer, sender_id).await?;
        let blocked_peer = self.services.users.is_blocked(sender_id, peer).await?;

        if blocked_by_peer || blocked_peer {
            return Err(DomainError::Blocked);
        }
        Ok(())
    }
}
