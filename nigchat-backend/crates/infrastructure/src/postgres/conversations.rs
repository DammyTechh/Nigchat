//! Conversations and messages — the hot path.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nigchat_domain::entities::*;
use nigchat_domain::ids::*;
use nigchat_domain::ports::{ConversationRepository, MessageRepository};
use nigchat_domain::values::{Cursor, MuteState, Seq};
use nigchat_domain::{DomainError, DomainResult};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

const MESSAGE_COLUMNS: &str = "id, conversation_id, seq, sender_id, sender_device_id, \
     client_message_id, kind, ciphertext, envelope_version, body_plaintext, metadata, \
     reply_to_id, forward_score, expires_at, edited_at, deleted_at, created_at";

#[derive(FromRow)]
struct ConversationRow {
    id: Uuid,
    kind: String,
    community_id: Option<Uuid>,
    title: Option<String>,
    description: Option<String>,
    avatar_media_id: Option<Uuid>,
    created_by: Option<Uuid>,
    only_admins_can_post: bool,
    disappearing_seconds: Option<i32>,
    max_members: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ConversationRow {
    fn into_entity(self) -> DomainResult<Conversation> {
        Ok(Conversation {
            id: ConversationId::from(self.id),
            kind: ConversationKind::parse(&self.kind)?,
            community_id: self.community_id.map(CommunityId::from),
            title: self.title,
            description: self.description,
            avatar_media_id: self.avatar_media_id.map(MediaId::from),
            created_by: self.created_by.map(UserId::from),
            only_admins_can_post: self.only_admins_can_post,
            disappearing_seconds: self.disappearing_seconds,
            max_members: self.max_members,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const CONVERSATION_COLUMNS: &str = "id, kind, community_id, title, description, avatar_media_id, \
     created_by, only_admins_can_post, disappearing_seconds, max_members, created_at, updated_at";

#[derive(FromRow)]
struct MessageRow {
    id: Uuid,
    conversation_id: Uuid,
    seq: i64,
    sender_id: Option<Uuid>,
    sender_device_id: Option<Uuid>,
    client_message_id: Uuid,
    kind: String,
    ciphertext: Option<Vec<u8>>,
    envelope_version: i16,
    body_plaintext: Option<String>,
    metadata: serde_json::Value,
    reply_to_id: Option<Uuid>,
    forward_score: i16,
    expires_at: Option<DateTime<Utc>>,
    edited_at: Option<DateTime<Utc>>,
    deleted_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl MessageRow {
    fn into_entity(self) -> DomainResult<Message> {
        Ok(Message {
            id: MessageId::from(self.id),
            conversation_id: ConversationId::from(self.conversation_id),
            seq: Seq(self.seq),
            sender_id: self.sender_id.map(UserId::from),
            sender_device_id: self.sender_device_id.map(DeviceId::from),
            client_message_id: ClientMessageId::from(self.client_message_id),
            kind: MessageKind::parse(&self.kind)?,
            ciphertext: self.ciphertext,
            envelope_version: self.envelope_version,
            system_text: self.body_plaintext,
            metadata: self.metadata,
            reply_to_id: self.reply_to_id.map(MessageId::from),
            forward_score: self.forward_score,
            expires_at: self.expires_at,
            edited_at: self.edited_at,
            deleted_at: self.deleted_at,
            created_at: self.created_at,
            // Filled in by `attach_media` for read paths; empty on write.
            attachments: Vec::new(),
        })
    }
}

/// Sorted pair, so the key is identical whichever user initiates.
fn direct_key(a: UserId, b: UserId) -> String {
    let (lo, hi) = if a.as_uuid() < b.as_uuid() {
        (a, b)
    } else {
        (b, a)
    };
    format!("{lo}:{hi}")
}

// --- conversations --------------------------------------------------------

pub struct PgConversationRepository {
    pool: PgPool,
}

impl PgConversationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConversationRepository for PgConversationRepository {
    async fn find_by_id(&self, id: ConversationId) -> DomainResult<Option<Conversation>> {
        let row = sqlx::query_as::<_, ConversationRow>(&format!(
            "SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.map(ConversationRow::into_entity).transpose()
    }

    /// Idempotent by construction: `ON CONFLICT (direct_key)` means two users
    /// tapping "message" simultaneously on different instances converge on one
    /// row instead of racing to create two.
    async fn get_or_create_direct(&self, a: UserId, b: UserId) -> DomainResult<Conversation> {
        let key = direct_key(a, b);
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let row = sqlx::query_as::<_, ConversationRow>(&format!(
            r#"
            INSERT INTO conversations (id, kind, direct_key, created_by)
            VALUES ($1, 'direct', $2, $3)
            ON CONFLICT (direct_key) DO UPDATE SET updated_at = conversations.updated_at
            RETURNING {CONVERSATION_COLUMNS}
            "#
        ))
        .bind(Uuid::now_v7())
        .bind(&key)
        .bind(a.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        sqlx::query("INSERT INTO conversation_counters (conversation_id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(row.id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;

        // `left_at = NULL` on conflict: re-opening a chat someone had left
        // restores them rather than failing.
        sqlx::query(
            r#"
            INSERT INTO conversation_members (conversation_id, user_id, role)
            VALUES ($1, $2, 'member'), ($1, $3, 'member')
            ON CONFLICT (conversation_id, user_id) DO UPDATE SET left_at = NULL
            "#,
        )
        .bind(row.id)
        .bind(a.as_uuid())
        .bind(b.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        row.into_entity()
    }

    async fn create_group(
        &self,
        creator: UserId,
        title: &str,
        description: Option<&str>,
        members: &[UserId],
    ) -> DomainResult<Conversation> {
        let id = Uuid::now_v7();
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let row = sqlx::query_as::<_, ConversationRow>(&format!(
            r#"
            INSERT INTO conversations (id, kind, title, description, created_by)
            VALUES ($1, 'group', $2, $3, $4)
            RETURNING {CONVERSATION_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(title)
        .bind(description)
        .bind(creator.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        sqlx::query("INSERT INTO conversation_counters (conversation_id) VALUES ($1)")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;

        sqlx::query(
            "INSERT INTO conversation_members (conversation_id, user_id, role) VALUES ($1, $2, 'owner')",
        )
        .bind(id)
        .bind(creator.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        // One statement for every member. The `SELECT ... FROM users` guard
        // silently skips ids that are not real accounts, so one stale contact
        // in the picker cannot fail the whole creation.
        let member_ids: Vec<Uuid> = members
            .iter()
            .copied()
            .filter(|id| *id != creator)
            .map(UserId::as_uuid)
            .collect();

        if !member_ids.is_empty() {
            sqlx::query(
                r#"
                INSERT INTO conversation_members (conversation_id, user_id, role, invited_by)
                SELECT $1, u.id, 'member', $3 FROM users u
                WHERE u.id = ANY($2) AND u.is_active
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(id)
            .bind(&member_ids)
            .bind(creator.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }

        sqlx::query(
            r#"
            INSERT INTO group_events (id, conversation_id, actor_id, event_type)
            VALUES ($1, $2, $3, 'created')
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(creator.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        row.into_entity()
    }

    /// The conversation list in one query. Two lateral joins pull the last
    /// message and, for direct chats, the peer's name and avatar, so rendering
    /// the list never needs a second round trip per row.
    async fn list_for_user(&self, user_id: UserId) -> DomainResult<Vec<ConversationSummary>> {
        #[derive(FromRow)]
        struct Row {
            id: Uuid,
            kind: String,
            title: Option<String>,
            avatar_media_id: Option<Uuid>,
            head_seq: i64,
            last_read_seq: i64,
            unread_count: i64,
            last_message_at: Option<DateTime<Utc>>,
            last_message_kind: Option<String>,
            is_pinned: bool,
            is_archived: bool,
            is_locked: bool,
            muted_until: Option<DateTime<Utc>>,
            updated_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT c.id,
                   c.kind,
                   CASE WHEN c.kind = 'direct' THEN peer.display_name ELSE c.title END AS title,
                   CASE WHEN c.kind = 'direct' THEN peer.avatar_media_id ELSE c.avatar_media_id END AS avatar_media_id,
                   COALESCE(cc.last_seq, 0) AS head_seq,
                   cm.last_read_seq,
                   GREATEST(COALESCE(cc.last_seq, 0) - cm.last_read_seq, 0) AS unread_count,
                   lm.created_at AS last_message_at,
                   lm.kind       AS last_message_kind,
                   cm.is_pinned,
                   cm.is_archived,
                   cm.is_locked,
                   cns.muted_until,
                   c.updated_at
            FROM conversation_members cm
            JOIN conversations c ON c.id = cm.conversation_id
            LEFT JOIN conversation_counters cc ON cc.conversation_id = c.id
            LEFT JOIN conversation_notification_settings cns
                   ON cns.conversation_id = c.id AND cns.user_id = cm.user_id
            LEFT JOIN LATERAL (
                SELECT m.created_at, m.kind
                FROM messages m
                WHERE m.conversation_id = c.id AND m.deleted_at IS NULL
                ORDER BY m.seq DESC
                LIMIT 1
            ) lm ON TRUE
            LEFT JOIN LATERAL (
                SELECT u.display_name, u.avatar_media_id
                FROM conversation_members cm2
                JOIN users u ON u.id = cm2.user_id
                WHERE cm2.conversation_id = c.id AND cm2.user_id <> $1
                LIMIT 1
            ) peer ON c.kind = 'direct'
            WHERE cm.user_id = $1 AND cm.left_at IS NULL
            ORDER BY cm.is_pinned DESC, COALESCE(lm.created_at, c.created_at) DESC
            LIMIT 500
            "#,
        )
        .bind(user_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        rows.into_iter()
            .map(|row| {
                Ok(ConversationSummary {
                    id: ConversationId::from(row.id),
                    kind: ConversationKind::parse(&row.kind)?,
                    title: row.title,
                    avatar_media_id: row.avatar_media_id.map(MediaId::from),
                    head_seq: Seq(row.head_seq),
                    last_read_seq: Seq(row.last_read_seq),
                    unread_count: row.unread_count,
                    last_message_at: row.last_message_at,
                    last_message_kind: row.last_message_kind,
                    is_pinned: row.is_pinned,
                    is_archived: row.is_archived,
                    is_locked: row.is_locked,
                    mute: MuteState {
                        muted_until: row.muted_until,
                    },
                    updated_at: row.updated_at,
                })
            })
            .collect()
    }

    async fn membership(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
    ) -> DomainResult<Option<ConversationMember>> {
        #[derive(FromRow)]
        struct Row {
            role: String,
            last_read_seq: i64,
            last_delivered_seq: i64,
            is_pinned: bool,
            is_archived: bool,
            is_locked: bool,
            joined_at: DateTime<Utc>,
            left_at: Option<DateTime<Utc>>,
        }

        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT role, last_read_seq, last_delivered_seq, is_pinned, is_archived,
                   is_locked, joined_at, left_at
            FROM conversation_members
            WHERE conversation_id = $1 AND user_id = $2
            "#,
        )
        .bind(conversation_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(|row| ConversationMember {
            conversation_id,
            user_id,
            role: MemberRole::parse(&row.role),
            last_read_seq: Seq(row.last_read_seq),
            last_delivered_seq: Seq(row.last_delivered_seq),
            is_pinned: row.is_pinned,
            is_archived: row.is_archived,
            is_locked: row.is_locked,
            joined_at: row.joined_at,
            left_at: row.left_at,
        }))
    }

    async fn active_member_ids(
        &self,
        conversation_id: ConversationId,
    ) -> DomainResult<Vec<UserId>> {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT user_id FROM conversation_members WHERE conversation_id = $1 AND left_at IS NULL",
        )
        .bind(conversation_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(ids.into_iter().map(UserId::from).collect())
    }

    async fn add_members(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        members: &[UserId],
    ) -> DomainResult<Vec<UserId>> {
        if members.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Uuid> = members.iter().copied().map(UserId::as_uuid).collect();

        // The capacity check lives in the same statement as the insert, so two
        // concurrent adds cannot both pass a check-then-insert and overshoot.
        let added: Vec<Uuid> = sqlx::query_scalar(
            r#"
            INSERT INTO conversation_members (conversation_id, user_id, role, invited_by)
            SELECT $1, u.id, 'member', $3
            FROM users u
            WHERE u.id = ANY($2)
              AND u.is_active
              AND (
                  SELECT COUNT(*) FROM conversation_members existing
                  WHERE existing.conversation_id = $1 AND existing.left_at IS NULL
              ) < (SELECT max_members FROM conversations WHERE id = $1)
            ON CONFLICT (conversation_id, user_id) DO UPDATE SET left_at = NULL
            RETURNING user_id
            "#,
        )
        .bind(conversation_id.as_uuid())
        .bind(&ids)
        .bind(actor.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(added.into_iter().map(UserId::from).collect())
    }

    /// Soft removal. Keeping the row preserves the read marker, so re-adding
    /// someone does not resurrect a thousand unread messages.
    async fn remove_member(
        &self,
        conversation_id: ConversationId,
        actor: UserId,
        target: UserId,
    ) -> DomainResult<()> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        sqlx::query(
            r#"
            UPDATE conversation_members SET left_at = now()
            WHERE conversation_id = $1 AND user_id = $2 AND left_at IS NULL
            "#,
        )
        .bind(conversation_id.as_uuid())
        .bind(target.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        sqlx::query(
            r#"
            INSERT INTO group_events (id, conversation_id, actor_id, target_id, event_type)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(conversation_id.as_uuid())
        .bind(actor.as_uuid())
        .bind(target.as_uuid())
        .bind(if actor == target {
            "member_left"
        } else {
            "member_removed"
        })
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok(())
    }

    async fn set_role(
        &self,
        conversation_id: ConversationId,
        target: UserId,
        role: MemberRole,
    ) -> DomainResult<()> {
        sqlx::query(
            "UPDATE conversation_members SET role = $3 WHERE conversation_id = $1 AND user_id = $2",
        )
        .bind(conversation_id.as_uuid())
        .bind(target.as_uuid())
        .bind(role.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    /// GREATEST means the marker only moves forward. A retried or reordered
    /// request from a laggy device cannot resurrect old unread counts.
    async fn advance_read_marker(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        seq: Seq,
    ) -> DomainResult<Seq> {
        let value: i64 = sqlx::query_scalar(
            r#"
            UPDATE conversation_members
            SET last_read_seq      = GREATEST(last_read_seq, $3),
                last_delivered_seq = GREATEST(last_delivered_seq, $3)
            WHERE conversation_id = $1 AND user_id = $2
            RETURNING last_read_seq
            "#,
        )
        .bind(conversation_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(seq.value())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(Seq(value))
    }

    async fn advance_delivery_marker(
        &self,
        conversation_id: ConversationId,
        user_id: UserId,
        seq: Seq,
    ) -> DomainResult<Seq> {
        let value: i64 = sqlx::query_scalar(
            r#"
            UPDATE conversation_members
            SET last_delivered_seq = GREATEST(last_delivered_seq, $3)
            WHERE conversation_id = $1 AND user_id = $2
            RETURNING last_delivered_seq
            "#,
        )
        .bind(conversation_id.as_uuid())
        .bind(user_id.as_uuid())
        .bind(seq.value())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(Seq(value))
    }

    async fn head_seq(&self, conversation_id: ConversationId) -> DomainResult<Seq> {
        let value: Option<i64> = sqlx::query_scalar(
            "SELECT last_seq FROM conversation_counters WHERE conversation_id = $1",
        )
        .bind(conversation_id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(Seq(value.unwrap_or(0)))
    }
}

// --- messages -------------------------------------------------------------

pub struct PgMessageRepository {
    pool: PgPool,
}

impl PgMessageRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Attaches media to a batch of already-loaded messages. Kept separate so
    /// the write path never pays for it.
    async fn hydrate_attachments(&self, messages: &mut [Message]) -> DomainResult<()> {
        if messages.is_empty() {
            return Ok(());
        }

        let ids: Vec<MessageId> = messages.iter().map(|m| m.id).collect();
        let attachments = self.attachments_for(&ids).await?;
        if attachments.is_empty() {
            return Ok(());
        }

        let mut grouped: std::collections::HashMap<MessageId, Vec<MessageAttachment>> =
            std::collections::HashMap::new();
        for (message_id, attachment) in attachments {
            grouped.entry(message_id).or_default().push(attachment);
        }

        for message in messages.iter_mut() {
            if let Some(found) = grouped.remove(&message.id) {
                message.attachments = found;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl MessageRepository for PgMessageRepository {
    /// Everything in one transaction: sequence allocation, the message, its
    /// mentions and attachments, and the outbox row.
    ///
    /// The sequence is allocated by `UPDATE ... RETURNING`, which takes a row
    /// lock on exactly one row of `conversation_counters`. Concurrent sends to
    /// *different* conversations never contend — the lock scope is one
    /// conversation.
    async fn append(&self, message: NewMessage) -> DomainResult<(Message, bool)> {
        // Fast path for a retry: if this client_message_id already exists we
        // return the original without consuming a sequence number.
        if let Some(existing) = self
            .find_by_client_id(
                message.conversation_id,
                message.sender_id,
                message.client_message_id,
            )
            .await?
        {
            return Ok((existing, false));
        }

        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let seq: i64 = sqlx::query_scalar(
            r#"
            UPDATE conversation_counters
            SET last_seq = last_seq + 1
            WHERE conversation_id = $1
            RETURNING last_seq
            "#,
        )
        .bind(message.conversation_id.as_uuid())
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(DomainError::not_found("conversation"))?;

        let id = Uuid::now_v7();
        let inserted = sqlx::query_as::<_, MessageRow>(&format!(
            r#"
            INSERT INTO messages
                (id, conversation_id, seq, sender_id, sender_device_id, client_message_id,
                 kind, ciphertext, envelope_version, metadata, reply_to_id, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            ON CONFLICT (conversation_id, sender_id, client_message_id) DO NOTHING
            RETURNING {MESSAGE_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(message.conversation_id.as_uuid())
        .bind(seq)
        .bind(message.sender_id.as_uuid())
        .bind(message.sender_device_id.as_uuid())
        .bind(message.client_message_id.0)
        .bind(message.kind.as_str())
        .bind(&message.ciphertext)
        .bind(message.envelope_version)
        .bind(&message.metadata)
        .bind(message.reply_to_id.map(MessageId::as_uuid))
        .bind(message.expires_at)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        let Some(row) = inserted else {
            // Two retries of the same client_message_id raced and the other
            // one won. Roll back this sequence allocation and return theirs —
            // a gap would be harmless but makes client sync harder to reason
            // about, so the rollback is worth it.
            tx.rollback().await.map_err(map_sqlx)?;

            let existing = self
                .find_by_client_id(
                    message.conversation_id,
                    message.sender_id,
                    message.client_message_id,
                )
                .await?
                .ok_or_else(|| DomainError::conflict("duplicate message send"))?;

            return Ok((existing, false));
        };

        if !message.mentions.is_empty() {
            let mention_ids: Vec<Uuid> = message.mentions.iter().copied().map(UserId::as_uuid).collect();
            sqlx::query(
                r#"
                INSERT INTO message_mentions (message_id, mentioned_user_id)
                SELECT $1, unnest($2::uuid[])
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(id)
            .bind(&mention_ids)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }

        for (position, media_id) in message.media_ids.iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO message_attachments (message_id, media_id, position)
                VALUES ($1, $2, $3) ON CONFLICT DO NOTHING
                "#,
            )
            .bind(id)
            .bind(media_id.as_uuid())
            .bind(position as i16)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;
        }

        sqlx::query("UPDATE conversations SET updated_at = now() WHERE id = $1")
            .bind(message.conversation_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;

        // Durability backstop, written in the same transaction as the message
        // so it can never disagree with what was stored.
        sqlx::query(
            r#"
            INSERT INTO event_outbox (topic, partition_key, payload)
            VALUES ('chat.message.created', $1, $2)
            "#,
        )
        .bind(message.conversation_id.to_string())
        .bind(serde_json::json!({
            "message_id": id,
            "conversation_id": message.conversation_id.as_uuid(),
            "seq": seq,
        }))
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok((row.into_entity()?, true))
    }

    async fn find_by_id(&self, id: MessageId) -> DomainResult<Option<Message>> {
        let row = sqlx::query_as::<_, MessageRow>(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM messages WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let Some(row) = row else { return Ok(None) };
        let mut message = row.into_entity()?;
        let mut batch = [message.clone()];
        self.hydrate_attachments(&mut batch).await?;
        message.attachments = batch[0].attachments.clone();

        Ok(Some(message))
    }

    async fn find_by_client_id(
        &self,
        conversation_id: ConversationId,
        sender_id: UserId,
        client_message_id: ClientMessageId,
    ) -> DomainResult<Option<Message>> {
        let row = sqlx::query_as::<_, MessageRow>(&format!(
            r#"
            SELECT {MESSAGE_COLUMNS} FROM messages
            WHERE conversation_id = $1 AND sender_id = $2 AND client_message_id = $3
            "#
        ))
        .bind(conversation_id.as_uuid())
        .bind(sender_id.as_uuid())
        .bind(client_message_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        row.map(MessageRow::into_entity).transpose()
    }

    /// Fetching `limit + 1` is how we know more exists without a COUNT.
    async fn page(
        &self,
        conversation_id: ConversationId,
        cursor: Cursor,
    ) -> DomainResult<(Vec<Message>, bool)> {
        let rows = if let Some(after) = cursor.after_seq {
            sqlx::query_as::<_, MessageRow>(&format!(
                r#"
                SELECT {MESSAGE_COLUMNS} FROM messages
                WHERE conversation_id = $1 AND seq > $2
                ORDER BY seq ASC LIMIT $3
                "#
            ))
            .bind(conversation_id.as_uuid())
            .bind(after.value())
            .bind(cursor.limit + 1)
            .fetch_all(&self.pool)
            .await
        } else {
            sqlx::query_as::<_, MessageRow>(&format!(
                r#"
                SELECT {MESSAGE_COLUMNS} FROM messages
                WHERE conversation_id = $1 AND ($2::bigint IS NULL OR seq < $2)
                ORDER BY seq DESC LIMIT $3
                "#
            ))
            .bind(conversation_id.as_uuid())
            .bind(cursor.before_seq.map(|seq| seq.value()))
            .bind(cursor.limit + 1)
            .fetch_all(&self.pool)
            .await
        }
        .map_err(map_sqlx)?;

        let has_more = rows.len() as i64 > cursor.limit;
        let mut messages = rows
            .into_iter()
            .take(cursor.limit as usize)
            .map(MessageRow::into_entity)
            .collect::<DomainResult<Vec<_>>>()?;

        self.hydrate_attachments(&mut messages).await?;

        Ok((messages, has_more))
    }

    async fn edit(
        &self,
        id: MessageId,
        editor: UserId,
        ciphertext: &[u8],
    ) -> DomainResult<Message> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        // Snapshot the previous version before overwriting, so edit history
        // survives (spec §4).
        sqlx::query(
            r#"
            INSERT INTO message_revisions (id, message_id, revision, ciphertext)
            SELECT $1, id, edit_count + 1, ciphertext FROM messages WHERE id = $2
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        let row = sqlx::query_as::<_, MessageRow>(&format!(
            r#"
            UPDATE messages
            SET ciphertext = $3, edited_at = now(), edit_count = edit_count + 1
            WHERE id = $1 AND sender_id = $2 AND deleted_at IS NULL
            RETURNING {MESSAGE_COLUMNS}
            "#
        ))
        .bind(id.as_uuid())
        .bind(editor.as_uuid())
        .bind(ciphertext)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(DomainError::Forbidden)?;

        tx.commit().await.map_err(map_sqlx)?;
        row.into_entity()
    }

    /// Soft delete: the row and its `seq` survive so other devices learn the
    /// message is gone rather than finding a hole in the sequence. The
    /// ciphertext is cleared immediately.
    async fn soft_delete(
        &self,
        id: MessageId,
        actor: UserId,
        for_everyone: bool,
    ) -> DomainResult<Seq> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let seq: i64 = sqlx::query_scalar(
            r#"
            UPDATE messages
            SET deleted_at = now(),
                deleted_for_everyone = $2,
                ciphertext = NULL,
                metadata = '{}'::jsonb
            WHERE id = $1 AND deleted_at IS NULL
            RETURNING seq
            "#,
        )
        .bind(id.as_uuid())
        .bind(for_everyone)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx)?
        .ok_or(DomainError::not_found("message"))?;

        let conversation_id: Uuid =
            sqlx::query_scalar("SELECT conversation_id FROM messages WHERE id = $1")
                .bind(id.as_uuid())
                .fetch_one(&mut *tx)
                .await
                .map_err(map_sqlx)?;

        // Tombstone, so a device that was offline during the deletion still
        // learns about it on next sync.
        sqlx::query(
            r#"
            INSERT INTO message_deletion_events
                (id, conversation_id, seq, deleted_by, for_everyone)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(Uuid::now_v7())
        .bind(conversation_id)
        .bind(seq)
        .bind(actor.as_uuid())
        .bind(for_everyone)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok(Seq(seq))
    }

    async fn set_reaction(
        &self,
        message_id: MessageId,
        user_id: UserId,
        emoji: &str,
        removed: bool,
    ) -> DomainResult<()> {
        if removed {
            sqlx::query(
                "DELETE FROM message_reactions WHERE message_id = $1 AND user_id = $2 AND emoji = $3",
            )
            .bind(message_id.as_uuid())
            .bind(user_id.as_uuid())
            .bind(emoji)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        } else {
            sqlx::query(
                r#"
                INSERT INTO message_reactions (message_id, user_id, emoji)
                VALUES ($1, $2, $3) ON CONFLICT DO NOTHING
                "#,
            )
            .bind(message_id.as_uuid())
            .bind(user_id.as_uuid())
            .bind(emoji)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        }
        Ok(())
    }

    /// One query for the whole page. `= ANY($1)` rather than a loop, because
    /// a 50-message page would otherwise be 51 round trips.
    async fn attachments_for(
        &self,
        message_ids: &[MessageId],
    ) -> DomainResult<Vec<(MessageId, MessageAttachment)>> {
        if message_ids.is_empty() {
            return Ok(Vec::new());
        }

        #[derive(FromRow)]
        struct Row {
            message_id: Uuid,
            media_id: Uuid,
            mime_type: String,
            byte_size: i64,
            width: Option<i32>,
            height: Option<i32>,
            duration_ms: Option<i32>,
            position: i16,
        }

        let ids: Vec<Uuid> = message_ids.iter().copied().map(MessageId::as_uuid).collect();

        let rows = sqlx::query_as::<_, Row>(
            r#"
            SELECT ma.message_id, ma.media_id, m.mime_type, m.byte_size,
                   m.width, m.height, m.duration_ms, ma.position
            FROM message_attachments ma
            JOIN media_assets m ON m.id = ma.media_id
            WHERE ma.message_id = ANY($1) AND m.upload_status = 'complete'
            ORDER BY ma.message_id, ma.position
            "#,
        )
        .bind(&ids)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (
                    MessageId::from(row.message_id),
                    MessageAttachment {
                        media_id: MediaId::from(row.media_id),
                        mime_type: row.mime_type,
                        byte_size: row.byte_size,
                        width: row.width,
                        height: row.height,
                        duration_ms: row.duration_ms,
                        position: row.position,
                    },
                )
            })
            .collect())
    }

    async fn mentioned_users(&self, message_id: MessageId) -> DomainResult<Vec<UserId>> {
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT mentioned_user_id FROM message_mentions WHERE message_id = $1",
        )
        .bind(message_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(ids.into_iter().map(UserId::from).collect())
    }
}
