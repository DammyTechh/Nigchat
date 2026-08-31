//! Notification dispatch (spec §16).
//!
//! The dispatcher gathers facts, hands them to the pure `NotificationPolicy`
//! in the domain, and carries out whatever the policy decided. It makes no
//! rules of its own — if you find yourself writing `if muted` here, the rule
//! belongs in `domain::notifications` where it can be unit-tested.
//!
//! Design points that matter at scale:
//!
//! * **Batched lookups.** Presence and block checks are resolved for the whole
//!   recipient set in one call each. A 500-member group must not become 1,500
//!   round trips.
//! * **Never on the request path.** Dispatch is spawned; a slow APNs
//!   connection must not add latency to the sender's HTTP response.
//! * **Suppressions are recorded.** "Why didn't I get notified?" is answerable
//!   from `notification_deliveries` with a reason, not a guess.
//! * **Idempotent.** The delivery row's unique constraint means a retried
//!   dispatch cannot buzz a phone twice.

use std::collections::HashSet;
use std::str::FromStr;

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use nigchat_domain::entities::{Conversation, Message, PushProvider};
use nigchat_domain::ids::{ConversationId, UserId};
use nigchat_domain::notifications::{
    NotificationCategory, NotificationContext, NotificationDecision, NotificationPlan,
    NotificationPolicy, SuppressionReason,
};
use nigchat_domain::ports::{PushMessage, PushOutcome};
use nigchat_domain::values::Seq;
use nigchat_domain::DomainResult;

use crate::services::Services;

pub struct NotificationDispatcher {
    services: Services,
}

pub struct NotifyMessageCommand {
    pub conversation: Conversation,
    pub message: Message,
    /// Every active member, including the sender — filtered here.
    pub recipients: Vec<UserId>,
    pub mentions: Vec<UserId>,
    /// Author of the message being replied to, if any.
    pub reply_to_author: Option<UserId>,
}

impl NotificationDispatcher {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    /// Fire-and-forget. Returns immediately; the work happens on a task.
    pub async fn notify_new_message(&self, command: NotifyMessageCommand) {
        let services = self.services.clone();
        tokio_spawn(async move {
            let dispatcher = NotificationDispatcher::new(services);
            if let Err(err) = dispatcher.dispatch_message(command).await {
                tracing::error!(?err, "notification dispatch failed");
            }
        });
    }

    async fn dispatch_message(&self, command: NotifyMessageCommand) -> DomainResult<()> {
        let sender_id = match command.message.sender_id {
            Some(id) => id,
            // System messages have no sender and no push.
            None => return Ok(()),
        };

        let audience: Vec<UserId> = command
            .recipients
            .iter()
            .copied()
            .filter(|id| *id != sender_id)
            .collect();

        if audience.is_empty() {
            return Ok(());
        }

        // Two batched queries for the whole audience instead of two per person.
        let online: HashSet<UserId> = self
            .services
            .presence
            .online_subset(&audience)
            .await?
            .into_iter()
            .collect();

        let blocked_sender: HashSet<UserId> = self
            .services
            .users
            .blocked_by_any(sender_id, &audience)
            .await?
            .into_iter()
            .collect();

        let mentioned: HashSet<UserId> = command.mentions.iter().copied().collect();

        let sender_name = self
            .services
            .users
            .find_by_id(sender_id)
            .await?
            .map(|user| user.display_name);

        let now = self.services.clock.now();

        for recipient in audience {
            let outcome = self
                .dispatch_to_recipient(
                    recipient,
                    &command,
                    sender_name.as_deref(),
                    mentioned.contains(&recipient),
                    command.reply_to_author == Some(recipient),
                    online.contains(&recipient),
                    blocked_sender.contains(&recipient),
                    now,
                )
                .await;

            // One recipient's failure must not abort the rest of the fan-out.
            if let Err(err) = outcome {
                tracing::warn!(?err, %recipient, "notification failed for recipient");
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn dispatch_to_recipient(
        &self,
        recipient: UserId,
        command: &NotifyMessageCommand,
        sender_name: Option<&str>,
        is_mention: bool,
        is_reply_to_recipient: bool,
        is_online: bool,
        sender_blocked: bool,
        now: DateTime<Utc>,
    ) -> DomainResult<()> {
        let preferences = self.services.notifications.preferences(recipient).await?;
        let settings = self
            .services
            .notifications
            .conversation_settings(command.conversation.id, recipient)
            .await?;

        let tokens = self.services.notifications.active_tokens(recipient).await?;

        let local_time = self.to_local_time(
            now,
            preferences
                .quiet_hours
                .as_ref()
                .map(|quiet| quiet.timezone.as_str()),
        );

        let context = NotificationContext {
            category: NotificationCategory::Message,
            conversation_kind: command.conversation.kind,
            message_kind: Some(command.message.kind),
            is_mention,
            is_reply_to_recipient,
            recipient_online: is_online,
            sender_blocked,
            is_own_message: false,
            has_valid_token: tokens.iter().any(|token| token.is_usable()),
            recipient_local_time: local_time,
            now_utc: now,
        };

        let decision = NotificationPolicy::decide(&context, &preferences, &settings);

        let plan = match decision {
            NotificationDecision::Suppress(reason) => {
                self.record(
                    recipient,
                    command.conversation.id,
                    command.message.seq,
                    NotificationCategory::Message,
                    "suppressed",
                    Some(reason.as_str()),
                    None,
                    None,
                )
                .await;
                tracing::debug!(%recipient, reason = reason.as_str(), "notification suppressed");
                return Ok(());
            }
            NotificationDecision::Send(plan) => *plan,
        };

        // The server cannot read the message (spec §28), so this is a template.
        // A device with `Full` preview decrypts locally and replaces the body.
        let title = NotificationPolicy::notification_title(
            &plan,
            sender_name,
            command.conversation.title.as_deref(),
            command.conversation.kind,
        );
        let body = NotificationPolicy::notification_body(&plan, Some(command.message.kind));

        for token in tokens.into_iter().filter(|token| token.is_usable()) {
            // A VoIP token is for incoming calls only. Sending a message push
            // through it on iOS is an API violation that gets the entitlement
            // revoked.
            if token.is_voip {
                continue;
            }

            let Some(sender) = self.services.push_for(token.provider) else {
                continue;
            };

            let push = PushMessage {
                token: token.token.clone(),
                provider: token.provider,
                title: title.clone(),
                body: body.clone(),
                plan: plan.clone(),
                deep_link: Some(format!(
                    "nigchat://conversation/{}",
                    command.conversation.id
                )),
                // Data-only fields the client needs to fetch and render the
                // real content. No plaintext, ever.
                data: serde_json::json!({
                    "conversation_id": command.conversation.id,
                    "message_id": command.message.id,
                    "seq": command.message.seq.value(),
                    "kind": command.message.kind.as_str(),
                    "tone_id": plan.tone_id,
                    "category": plan.category.as_str(),
                }),
                badge: None,
                is_voip: false,
                sandbox: token.sandbox,
            };

            match sender.send(push).await {
                Ok(PushOutcome::Delivered {
                    provider_message_id,
                }) => {
                    self.record(
                        recipient,
                        command.conversation.id,
                        command.message.seq,
                        plan.category,
                        "sent",
                        None,
                        Some(token.provider),
                        provider_message_id.as_deref(),
                    )
                    .await;
                }
                Ok(PushOutcome::TokenInvalid) => {
                    // The user uninstalled, or the token rotated. Mark it dead
                    // so we stop paying to push into a void (spec §16:
                    // invalid-token cleanup).
                    tracing::info!(%recipient, "push token invalid; retiring it");
                    self.services
                        .notifications
                        .invalidate_token(&token.token)
                        .await
                        .ok();
                }
                Ok(PushOutcome::Retryable(reason)) => {
                    self.services
                        .notifications
                        .record_token_failure(&token.token)
                        .await
                        .ok();
                    tracing::warn!(%recipient, reason, "push retryable failure");
                }
                Ok(PushOutcome::Failed(reason)) | Err(nigchat_domain::DomainError::Infrastructure(reason)) => {
                    self.record(
                        recipient,
                        command.conversation.id,
                        command.message.seq,
                        plan.category,
                        "failed",
                        None,
                        Some(token.provider),
                        Some(&reason),
                    )
                    .await;
                }
                Err(err) => {
                    tracing::warn!(?err, %recipient, "push send error");
                }
            }
        }

        Ok(())
    }

    /// Converts UTC into the recipient's own wall clock.
    ///
    /// Quiet hours are stored as local minutes plus an IANA zone precisely so
    /// that "quiet from 22:00" survives the user travelling and the clocks
    /// changing. An unknown or missing zone falls back to UTC — the window is
    /// then wrong for that user, which is far better than failing the send.
    fn to_local_time(&self, now: DateTime<Utc>, timezone: Option<&str>) -> DateTime<FixedOffset> {
        let Some(timezone) = timezone else {
            return now.fixed_offset();
        };

        match chrono_tz::Tz::from_str(timezone) {
            Ok(tz) => tz.from_utc_datetime(&now.naive_utc()).fixed_offset(),
            Err(_) => {
                tracing::warn!(timezone, "unknown timezone; falling back to UTC");
                now.fixed_offset()
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn record(
        &self,
        user_id: UserId,
        conversation_id: ConversationId,
        seq: Seq,
        category: NotificationCategory,
        status: &str,
        suppressed_reason: Option<&str>,
        provider: Option<PushProvider>,
        detail: Option<&str>,
    ) {
        let result = self
            .services
            .notifications
            .record_delivery(
                user_id,
                None,
                Some(conversation_id),
                Some(seq),
                category.as_str(),
                status,
                suppressed_reason,
                provider.map(|p| p.as_str()),
                detail,
            )
            .await;

        if let Err(err) = result {
            // Ledger failures are diagnostics, never a reason to drop a
            // notification the policy already approved.
            tracing::warn!(?err, "failed to record notification delivery");
        }
    }
}

/// Spawned rather than awaited: a slow APNs connection must never add latency
/// to the sender's HTTP response. Tokio is the only runtime dependency the
/// application layer takes, and it is confined to this call.
fn tokio_spawn<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}
