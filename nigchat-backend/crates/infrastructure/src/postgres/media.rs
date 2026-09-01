//! Media metadata. Bytes live in object storage; only the record is here.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nigchat_domain::ids::{MediaId, UserId};
use nigchat_domain::ports::{MediaAsset, MediaRepository, NewMedia};
use nigchat_domain::DomainResult;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::map_sqlx;

const COLUMNS: &str = "id, owner_id, storage_bucket, storage_key, mime_type, byte_size, \
     width, height, duration_ms, is_encrypted, upload_status, created_at";

#[derive(FromRow)]
struct Row {
    id: Uuid,
    owner_id: Option<Uuid>,
    storage_bucket: String,
    storage_key: String,
    mime_type: String,
    byte_size: i64,
    width: Option<i32>,
    height: Option<i32>,
    duration_ms: Option<i32>,
    is_encrypted: bool,
    upload_status: String,
    created_at: DateTime<Utc>,
}

impl From<Row> for MediaAsset {
    fn from(row: Row) -> Self {
        MediaAsset {
            id: MediaId::from(row.id),
            owner_id: row.owner_id.map(UserId::from),
            bucket: row.storage_bucket,
            key: row.storage_key,
            mime_type: row.mime_type,
            byte_size: row.byte_size,
            width: row.width,
            height: row.height,
            duration_ms: row.duration_ms,
            is_encrypted: row.is_encrypted,
            upload_status: row.upload_status,
            created_at: row.created_at,
        }
    }
}

pub struct PgMediaRepository {
    pool: PgPool,
}

impl PgMediaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MediaRepository for PgMediaRepository {
    async fn create_pending(&self, media: NewMedia) -> DomainResult<MediaAsset> {
        let row = sqlx::query_as::<_, Row>(&format!(
            r#"
            INSERT INTO media_assets
                (id, owner_id, storage_bucket, storage_key, mime_type, byte_size,
                 width, height, duration_ms, is_encrypted, upload_status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, 'pending')
            RETURNING {COLUMNS}
            "#
        ))
        .bind(Uuid::now_v7())
        .bind(media.owner_id.as_uuid())
        .bind(&media.bucket)
        .bind(&media.key)
        .bind(&media.mime_type)
        .bind(media.byte_size)
        .bind(media.width)
        .bind(media.height)
        .bind(media.duration_ms)
        .bind(media.is_encrypted)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.into())
    }

    async fn find(&self, id: MediaId) -> DomainResult<Option<MediaAsset>> {
        let row = sqlx::query_as::<_, Row>(&format!(
            "SELECT {COLUMNS} FROM media_assets WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(row.map(Into::into))
    }

    /// Ownership is in the predicate, not checked separately — one statement,
    /// no window where another request could change the row in between.
    async fn mark_complete(
        &self,
        id: MediaId,
        owner: UserId,
        byte_size: i64,
    ) -> DomainResult<bool> {
        let result = sqlx::query(
            r#"
            UPDATE media_assets
            SET upload_status = 'complete', byte_size = $3, completed_at = now()
            WHERE id = $1 AND owner_id = $2 AND upload_status = 'pending'
            "#,
        )
        .bind(id.as_uuid())
        .bind(owner.as_uuid())
        .bind(byte_size)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(result.rows_affected() == 1)
    }

    async fn stale_pending(&self, older_than_minutes: i64) -> DomainResult<Vec<MediaAsset>> {
        let rows = sqlx::query_as::<_, Row>(&format!(
            r#"
            SELECT {COLUMNS} FROM media_assets
            WHERE upload_status = 'pending'
              AND created_at < now() - ($1 || ' minutes')::interval
            LIMIT 500
            "#
        ))
        .bind(older_than_minutes.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx)?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn delete(&self, id: MediaId) -> DomainResult<()> {
        sqlx::query("DELETE FROM media_assets WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx)?;
        Ok(())
    }
}
