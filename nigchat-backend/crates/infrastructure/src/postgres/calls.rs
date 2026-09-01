//! Call sessions. Signalling metadata only — never media.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nigchat_domain::ids::{CallId, ConversationId, UserId};
use nigchat_domain::ports::{CallRepository, CallSession};
use nigchat_domain::DomainResult;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

pub struct PgCallRepository {
    pool: PgPool,
}

impl PgCallRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    async fn participants(&self, call_id: CallId) -> DomainResult<Vec<UserId>> {
        let ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT user_id FROM call_participants WHERE call_id = $1")
                .bind(call_id.as_uuid())
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx)?;
        Ok(ids.into_iter().map(UserId::from).collect())
    }
}

#[derive(FromRow)]
struct Row {
    id: Uuid,
    conversation_id: Option<Uuid>,
    initiator_id: Option<Uuid>,
    kind: String,
    is_group: bool,
    sfu_room_id: Option<String>,
    started_at: DateTime<Utc>,
    answered_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
    end_reason: Option<String>,
}

impl Row {
    fn into_session(self, participants: Vec<UserId>) -> CallSession {
        CallSession {
            id: CallId::from(self.id),
            conversation_id: self.conversation_id.map(ConversationId::from),
            initiator_id: self.initiator_id.map(UserId::from),
            kind: self.kind,
            is_group: self.is_group,
            room: self.sfu_room_id.unwrap_or_default(),
            started_at: self.started_at,
            answered_at: self.answered_at,
            ended_at: self.ended_at,
            end_reason: self.end_reason,
            participants,
        }
    }
}

const COLUMNS: &str = "id, conversation_id, initiator_id, kind, is_group, sfu_room_id, \
     started_at, answered_at, ended_at, end_reason";

#[async_trait]
impl CallRepository for PgCallRepository {
    async fn start(
        &self,
        conversation_id: Option<ConversationId>,
        initiator: UserId,
        kind: &str,
        is_group: bool,
        room: &str,
        participants: &[UserId],
    ) -> DomainResult<CallSession> {
        let id = Uuid::now_v7();
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let row = sqlx::query_as::<_, Row>(&format!(
            r#"
            INSERT INTO calls (id, conversation_id, initiator_id, kind, is_group, sfu_room_id)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING {COLUMNS}
            "#
        ))
        .bind(id)
        .bind(conversation_id.map(ConversationId::as_uuid))
        .bind(initiator.as_uuid())
        .bind(kind)
        .bind(is_group)
        .bind(room)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        // The participant rows are the guest list. Written in the same
        // transaction as the call, so a call can never exist that nobody is
        // permitted to join.
        let ids: Vec<Uuid> = participants.iter().copied().map(UserId::as_uuid).collect();

        sqlx::query(
            r#"
            INSERT INTO call_participants (call_id, user_id, state, joined_at)
            SELECT $1, u.id,
                   CASE WHEN u.id = $3 THEN 'joined' ELSE 'ringing' END,
                   CASE WHEN u.id = $3 THEN now() ELSE NULL END
            FROM unnest($2::uuid[]) AS u(id)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&ids)
        .bind(initiator.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;

        Ok(row.into_session(participants.to_vec()))
    }

    async fn find(&self, id: CallId) -> DomainResult<Option<CallSession>> {
        let row = sqlx::query_as::<_, Row>(&format!("SELECT {COLUMNS} FROM calls WHERE id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx)?;

        let Some(row) = row else { return Ok(None) };
        let participants = self.participants(id).await?;
        Ok(Some(row.into_session(participants)))
    }

    /// The `WHERE` clause is the authorisation: only someone already on the
    /// guest list can move to `joined`.
    async fn mark_joined(&self, call_id: CallId, user_id: UserId) -> DomainResult<bool> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        let result = sqlx::query(
            r#"
            UPDATE call_participants
            SET state = 'joined', joined_at = COALESCE(joined_at, now())
            WHERE call_id = $1 AND user_id = $2 AND state IN ('ringing', 'waiting_room', 'joined')
            "#,
        )
        .bind(call_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        // First answer is what "answered" means, so it is set once.
        sqlx::query("UPDATE calls SET answered_at = COALESCE(answered_at, now()) WHERE id = $1")
            .bind(call_id.as_uuid())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok(result.rows_affected() == 1)
    }

    async fn mark_left(&self, call_id: CallId, user_id: UserId) -> DomainResult<()> {
        sqlx::query(
            "UPDATE call_participants SET state = 'left', left_at = now() WHERE call_id = $1 AND user_id = $2",
        )
        .bind(call_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;
        Ok(())
    }

    async fn end(&self, call_id: CallId, reason: &str) -> DomainResult<Vec<UserId>> {
        let participants = self.participants(call_id).await?;
        let mut tx = self.pool.begin().await.map_err(map_sqlx)?;

        // `ended_at IS NULL` so a second hang-up does not overwrite the reason
        // recorded by the first.
        sqlx::query(
            "UPDATE calls SET ended_at = now(), end_reason = $2 WHERE id = $1 AND ended_at IS NULL",
        )
        .bind(call_id.as_uuid())
        .bind(reason)
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        sqlx::query(
            r#"
            UPDATE call_participants
            SET state = CASE WHEN state = 'ringing' THEN 'missed' ELSE 'left' END,
                left_at = COALESCE(left_at, now())
            WHERE call_id = $1 AND state <> 'left'
            "#,
        )
        .bind(call_id.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx)?;

        tx.commit().await.map_err(map_sqlx)?;
        Ok(participants)
    }

    async fn history(&self, user_id: UserId, limit: i64) -> DomainResult<Vec<CallSession>> {
        let rows = sqlx::query_as::<_, Row>(&format!(
            r#"
            SELECT {COLUMNS} FROM calls c
            WHERE EXISTS (
                SELECT 1 FROM call_participants p
                WHERE p.call_id = c.id AND p.user_id = $1
            )
            ORDER BY c.started_at DESC
            LIMIT $2
            "#
        ))
        .bind(user_id.as_uuid())
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            let id = CallId::from(row.id);
            let participants = self.participants(id).await?;
            sessions.push(row.into_session(participants));
        }

        Ok(sessions)
    }
}
