//! Conversation use cases (spec §6, §7, §8, §26.3).

use nigchat_domain::entities::{
    Conversation, ConversationKind, ConversationMember, ConversationSummary, MemberRole, Visibility,
};
use nigchat_domain::events::{EventEnvelope, MembershipChange, ServerEvent};
use nigchat_domain::ids::{ConversationId, UserId};
use nigchat_domain::values::{MuteDuration, MuteState, Seq};
use nigchat_domain::{DomainError, DomainResult};

use crate::services::Services;

const MAX_GROUP_TITLE_CHARS: usize = 100;

pub struct ConversationService {
    services: Services,
}

impl ConversationService {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    pub async fn list(&self, user_id: UserId) -> DomainResult<Vec<ConversationSummary>> {
        self.services.conversations.list_for_user(user_id).await
    }

    /// Idempotent: two users tapping "message" at the same moment on different
    /// instances end up in one conversation, not two. The uniqueness is
    /// enforced in the database by `direct_key`, not by a check-then-insert
    /// here, which would race.
    pub async fn open_direct(
        &self,
        caller: UserId,
        peer: UserId,
    ) -> DomainResult<Conversation> {
        if caller == peer {
            return Err(DomainError::validation(
                "cannot open a direct conversation with yourself",
            ));
        }

        let peer_user = self
            .services
            .users
            .find_by_id(peer)
            .await?
            .filter(|user| user.is_active)
            .ok_or(DomainError::not_found("user"))?;

        // A blocked user must not be able to open a channel to the person who
        // blocked them — including by creating the conversation row.
        if self.services.users.is_blocked(peer_user.id, caller).await? {
            return Err(DomainError::Blocked);
        }

        self.services
            .conversations
            .get_or_create_direct(caller, peer)
            .await
    }

    pub async fn create_group(
        &self,
        creator: UserId,
        title: &str,
        description: Option<&str>,
        members: &[UserId],
    ) -> DomainResult<Conversation> {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > MAX_GROUP_TITLE_CHARS {
            return Err(DomainError::validation(
                "group title must be between 1 and 100 characters",
            ));
        }

        self.services
            .rate_limiter
            .check(&format!("group:create:{creator}"), 20, 3_600)
            .await?;

        let allowed = self.filter_invitable(creator, members).await?;

        let conversation = self
            .services
            .conversations
            .create_group(creator, title, description, &allowed)
            .await?;

        let recipients = self
            .services
            .conversations
            .active_member_ids(conversation.id)
            .await?;

        self.services
            .events
            .publish(EventEnvelope::broadcast(
                recipients,
                ServerEvent::ConversationCreated {
                    conversation_id: conversation.id,
                    kind: conversation.kind.as_str().to_string(),
                },
            ))
            .await
            .ok();

        Ok(conversation)
    }

    pub async fn get(
        &self,
        conversation_id: ConversationId,
        caller: UserId,
    ) -> DomainResult<Conversation> {
        self.membership(conversation_id, caller).await?;
        self.services
            .conversations
            .find_by_id(conversation_id)
            .await?
            .ok_or(DomainError::not_found("conversation"))
    }

    pub async fn add_members(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        members: &[UserId],
    ) -> DomainResult<Vec<UserId>> {
        let membership = self.membership(conversation_id, actor).await?;
        let conversation = self
            .services
            .conversations
            .find_by_id(conversation_id)
            .await?
            .ok_or(DomainError::not_found("conversation"))?;

        if conversation.kind == ConversationKind::Direct {
            return Err(DomainError::validation(
                "members cannot be added to a direct conversation",
            ));
        }
        if !membership.role.can_administer() {
            return Err(DomainError::Forbidden);
        }

        // Applied on this path too, not just at creation — otherwise the rule
        // is trivially bypassed by creating an empty group and then adding.
        let allowed = self.filter_invitable(actor, members).await?;

        let added = self
            .services
            .conversations
            .add_members(conversation_id, actor, &allowed)
            .await?;

        let recipients = self
            .services
            .conversations
            .active_member_ids(conversation_id)
            .await?;

        for user_id in &added {
            self.services
                .events
                .publish(EventEnvelope::broadcast(
                    recipients.clone(),
                    ServerEvent::MembershipChanged {
                        conversation_id,
                        user_id: *user_id,
                        change: MembershipChange::Joined,
                    },
                ))
                .await
                .ok();
        }

        Ok(added)
    }

    /// Removing someone else requires admin rights; anyone may remove
    /// themselves, which is how "leave group" is expressed.
    pub async fn remove_member(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        target: UserId,
    ) -> DomainResult<()> {
        let membership = self.membership(conversation_id, actor).await?;
        let is_self = actor == target;

        if !is_self && !membership.role.can_administer() {
            return Err(DomainError::Forbidden);
        }

        // An owner cannot be removed by an admin — only by themselves.
        if !is_self {
            if let Some(target_membership) = self
                .services
                .conversations
                .membership(conversation_id, target)
                .await?
            {
                if target_membership.role == MemberRole::Owner {
                    return Err(DomainError::Forbidden);
                }
            }
        }

        // Recipients captured before removal so the leaver's own devices are
        // told to drop the conversation.
        let recipients = self
            .services
            .conversations
            .active_member_ids(conversation_id)
            .await?;

        self.services
            .conversations
            .remove_member(conversation_id, actor, target)
            .await?;

        self.services
            .events
            .publish(EventEnvelope::broadcast(
                recipients,
                ServerEvent::MembershipChanged {
                    conversation_id,
                    user_id: target,
                    change: if is_self {
                        MembershipChange::Left
                    } else {
                        MembershipChange::Removed
                    },
                },
            ))
            .await
            .ok();

        Ok(())
    }

    pub async fn set_role(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        target: UserId,
        role: MemberRole,
    ) -> DomainResult<()> {
        let membership = self.membership(conversation_id, actor).await?;

        // Only an owner may create or demote another owner; an admin can only
        // manage plain members.
        let permitted = match role {
            MemberRole::Owner => membership.role == MemberRole::Owner,
            _ => membership.role.can_administer(),
        };
        if !permitted {
            return Err(DomainError::Forbidden);
        }

        self.services
            .conversations
            .set_role(conversation_id, target, role)
            .await?;

        let recipients = self
            .services
            .conversations
            .active_member_ids(conversation_id)
            .await?;

        self.services
            .events
            .publish(EventEnvelope::broadcast(
                recipients,
                ServerEvent::MembershipChanged {
                    conversation_id,
                    user_id: target,
                    change: MembershipChange::RoleChanged,
                },
            ))
            .await
            .ok();

        Ok(())
    }

    /// Mute is per member, per conversation. "Always" is a far-future
    /// timestamp rather than a null, so one comparison covers every case.
    pub async fn mute(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        duration: Option<MuteDuration>,
    ) -> DomainResult<MuteState> {
        self.membership(conversation_id, user_id).await?;

        let mut settings = self
            .services
            .notifications
            .conversation_settings(conversation_id, user_id)
            .await?;

        settings.mute = MuteState {
            muted_until: duration.map(|duration| duration.until(self.services.clock.now())),
        };

        self.services
            .notifications
            .save_conversation_settings(&settings)
            .await?;

        Ok(settings.mute)
    }

    pub async fn head_seq(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<Seq> {
        self.membership(conversation_id, user_id).await?;
        self.services.conversations.head_seq(conversation_id).await
    }

    /// Honours each invitee's "who can add me to groups" setting (spec §14)
    /// and drops anyone who has blocked the inviter. Silently skipping is
    /// deliberate: telling the inviter *which* contacts refused would leak
    /// those users' privacy settings.
    async fn filter_invitable(
        &self,
        inviter: UserId,
        candidates: &[UserId],
    ) -> DomainResult<Vec<UserId>> {
        let mut allowed = Vec::with_capacity(candidates.len());

        for candidate in candidates.iter().filter(|id| **id != inviter) {
            if self.services.users.is_blocked(*candidate, inviter).await? {
                continue;
            }

            let privacy = self.services.users.privacy_settings(*candidate).await?;
            let permitted = match privacy.who_can_add_to_groups {
                Visibility::Everyone => true,
                // "Contacts only" cannot be resolved server-side without a
                // contact graph, which is exactly what E2EE contact hashing
                // avoids building. Treated as permitted; the client shows the
                // invite for confirmation.
                Visibility::Contacts => true,
                Visibility::Nobody => false,
            };

            if permitted {
                allowed.push(*candidate);
            }
        }

        Ok(allowed)
    }

    async fn membership(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<ConversationMember> {
        self.services
            .conversations
            .membership(conversation_id, user_id)
            .await?
            .filter(|member| member.is_active())
            // Identical error for "no such conversation" and "not a member":
            // probing must not reveal that a conversation exists.
            .ok_or(DomainError::Forbidden)
    }
}
