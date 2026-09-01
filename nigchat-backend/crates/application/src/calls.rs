//! Calls (spec §29).
//!
//! This service is the *signalling* half: who is calling whom, who answered,
//! when it ended. The audio and video go through an SFU — LiveKit — which the
//! participants connect to directly.
//!
//! That split is not a shortcut, it is the only shape that works. Media through
//! this service would mean every call's bandwidth flowing through an API
//! worker, and a three-person call needs the server to forward each stream to
//! each other participant. That is what an SFU is for.
//!
//! ```text
//!   caller  POST /v1/calls              -> { call_id, room, token, server_url }
//!           connects to the SFU with the token
//!   callee  receives `call_signal` over the socket, and a high-priority push
//!           POST /v1/calls/{id}/join    -> { token, server_url }
//!           connects to the same room
//! ```
//!
//! The token is minted per participant, scoped to one room, and expires in ten
//! minutes. Knowing a room name is not enough to enter it.

use nigchat_domain::entities::ConversationKind;
use nigchat_domain::events::{CallSignalKind, EventEnvelope, ServerEvent};
use nigchat_domain::ids::{CallId, ConversationId, UserId};
use nigchat_domain::ports::CallSession;
use nigchat_domain::{DomainError, DomainResult};
use uuid::Uuid;

use crate::services::Services;

pub struct CallService {
    services: Services,
}

pub struct CallTicket {
    pub call: CallSession,
    /// LiveKit access token for this participant, this room.
    pub token: String,
    pub server_url: String,
}

impl CallService {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    /// Starts a call and rings everyone else in the conversation.
    pub async fn start(
        &self,
        caller: UserId,
        conversation_id: ConversationId,
        video: bool,
    ) -> DomainResult<CallTicket> {
        let tokens = self
            .services
            .media_server
            .as_ref()
            .ok_or_else(|| DomainError::infrastructure("calling is not configured"))?;

        let conversation = self
            .services
            .conversations
            .find_by_id(conversation_id)
            .await?
            .ok_or(DomainError::Forbidden)?;

        // Membership check first — the same Forbidden either way, so probing
        // cannot reveal that a conversation exists.
        self.services
            .conversations
            .membership(conversation_id, caller)
            .await?
            .filter(|member| member.is_active())
            .ok_or(DomainError::Forbidden)?;

        let members = self
            .services
            .conversations
            .active_member_ids(conversation_id)
            .await?;

        // A block stops a call in both directions, exactly as it stops a
        // message. Checked before anything is created.
        if conversation.kind == ConversationKind::Direct {
            if let Some(peer) = members.iter().copied().find(|id| *id != caller) {
                let blocked = self.services.users.is_blocked(peer, caller).await?
                    || self.services.users.is_blocked(caller, peer).await?;
                if blocked {
                    return Err(DomainError::Blocked);
                }

                // "Who can call me" is a real setting, so honour it.
                let privacy = self.services.users.privacy_settings(peer).await?;
                if matches!(
                    privacy.who_can_call,
                    nigchat_domain::entities::Visibility::Nobody
                ) {
                    return Err(DomainError::Forbidden);
                }
            }
        }

        // Ringing people is expensive and noisy. This is an abuse ceiling.
        self.services
            .rate_limiter
            .check(&format!("call:start:{caller}"), 30, 3_600)
            .await?;

        // Unguessable, so the room name itself leaks nothing and cannot be
        // joined by someone who merely saw it.
        let room = format!("call-{}", Uuid::new_v4());
        let kind = if video { "video" } else { "audio" };
        let is_group = conversation.kind != ConversationKind::Direct;

        let call = self
            .services
            .calls
            .start(
                Some(conversation_id),
                caller,
                kind,
                is_group,
                &room,
                &members,
            )
            .await?;

        let token = self.token_for(tokens.as_ref(), &room, caller).await?;

        // Ring everyone else. The socket is the fast path; push is what wakes a
        // locked phone, and the notification policy already treats a call as
        // high priority and lets it through quiet hours.
        let others: Vec<UserId> = members.into_iter().filter(|id| *id != caller).collect();

        if !others.is_empty() {
            self.services
                .events
                .publish(EventEnvelope::broadcast(
                    others,
                    ServerEvent::CallSignal {
                        call_id: call.id,
                        conversation_id: Some(conversation_id),
                        signal: CallSignalKind::Ringing,
                        payload: serde_json::json!({
                            "kind": kind,
                            "from": caller,
                            "is_group": is_group,
                        }),
                    },
                ))
                .await
                .ok();
        }

        Ok(CallTicket {
            call,
            token,
            server_url: tokens.server_url().to_string(),
        })
    }

    /// Answering. Returns a token scoped to the same room.
    pub async fn join(&self, user: UserId, call_id: CallId) -> DomainResult<CallTicket> {
        let tokens = self
            .services
            .media_server
            .as_ref()
            .ok_or_else(|| DomainError::infrastructure("calling is not configured"))?;

        let call = self
            .services
            .calls
            .find(call_id)
            .await?
            .ok_or(DomainError::not_found("call"))?;

        if !call.is_active() {
            return Err(DomainError::validation("this call has ended"));
        }

        // The participant list is the guest list. Knowing a call id is not
        // enough — you have to have been invited.
        if !self.services.calls.mark_joined(call_id, user).await? {
            return Err(DomainError::Forbidden);
        }

        let token = self.token_for(tokens.as_ref(), &call.room, user).await?;

        self.services
            .events
            .publish(
                EventEnvelope::broadcast(
                    call.participants.clone(),
                    ServerEvent::CallSignal {
                        call_id,
                        conversation_id: call.conversation_id,
                        signal: CallSignalKind::Accepted,
                        payload: serde_json::json!({ "user_id": user }),
                    },
                )
                .excluding(user),
            )
            .await
            .ok();

        Ok(CallTicket {
            call,
            token,
            server_url: tokens.server_url().to_string(),
        })
    }

    /// Declining, hanging up, or the caller cancelling before an answer.
    pub async fn end(&self, user: UserId, call_id: CallId, reason: &str) -> DomainResult<()> {
        let call = self
            .services
            .calls
            .find(call_id)
            .await?
            .ok_or(DomainError::not_found("call"))?;

        if !call.participants.contains(&user) {
            return Err(DomainError::Forbidden);
        }

        // In a group, one person leaving is not the end of the call.
        if call.is_group && reason == "left" {
            self.services.calls.mark_left(call_id, user).await?;

            self.services
                .events
                .publish(
                    EventEnvelope::broadcast(
                        call.participants.clone(),
                        ServerEvent::CallSignal {
                            call_id,
                            conversation_id: call.conversation_id,
                            signal: CallSignalKind::ParticipantLeft,
                            payload: serde_json::json!({ "user_id": user }),
                        },
                    )
                    .excluding(user),
                )
                .await
                .ok();

            return Ok(());
        }

        let participants = self.services.calls.end(call_id, reason).await?;

        // Everyone's phone must stop ringing, including devices that never
        // answered.
        self.services
            .events
            .publish(EventEnvelope::broadcast(
                participants,
                ServerEvent::CallSignal {
                    call_id,
                    conversation_id: call.conversation_id,
                    signal: CallSignalKind::Ended,
                    payload: serde_json::json!({ "reason": reason, "by": user }),
                },
            ))
            .await
            .ok();

        Ok(())
    }

    pub async fn history(&self, user: UserId, limit: i64) -> DomainResult<Vec<CallSession>> {
        self.services
            .calls
            .history(user, limit.clamp(1, 200))
            .await
    }

    async fn token_for(
        &self,
        tokens: &dyn nigchat_domain::ports::MediaServerTokens,
        room: &str,
        user: UserId,
    ) -> DomainResult<String> {
        // The display name is what the other participants see in the call UI.
        let name = self
            .services
            .users
            .find_by_id(user)
            .await?
            .map(|user| user.display_name)
            .unwrap_or_else(|| "NigChat user".to_string());

        tokens.issue(room, &user.to_string(), &name)
    }
}
