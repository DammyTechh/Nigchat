//! Media uploads (spec §25, Appendix 3).
//!
//! Bytes never pass through this service:
//!
//! ```text
//!   client  POST /v1/media/uploads       -> { media_id, upload_url }
//!           PUT  <upload_url>            -> straight to object storage
//!           POST /v1/media/{id}/complete -> the row flips to `complete`
//! ```
//!
//! Routing a 40 MB video through an API worker would occupy it for the whole
//! upload on a mobile connection. A handful of concurrent uploads would starve
//! the workers that are supposed to be delivering messages, and autoscaling on
//! CPU would not notice, because the workers are idle — just blocked.
//!
//! Two consequences worth knowing:
//!
//! * The server never sees the bytes, so it cannot verify what was actually
//!   uploaded. `byte_size` and `mime_type` are declared by the client and
//!   enforced by the storage layer's own limits, not by us.
//! * An upload can be started and abandoned. Those rows stay `pending` and the
//!   sweeper deletes them — otherwise every cancelled upload is storage you pay
//!   for indefinitely.

use nigchat_domain::ids::{MediaId, UserId};
use nigchat_domain::ports::{MediaAsset, NewMedia, SignedUpload};
use nigchat_domain::{DomainError, DomainResult};
use uuid::Uuid;

use crate::services::Services;

/// Avatars are small and are re-encoded by the client before upload. A generous
/// ceiling still rejects someone PUTting a raw 40-megapixel original.
const MAX_AVATAR_BYTES: i64 = 8 * 1024 * 1024;

/// Conversation attachments. Above this, the client should be asked to compress.
const MAX_ATTACHMENT_BYTES: i64 = 100 * 1024 * 1024;

/// Uploads that never completed are swept after this long.
pub const STALE_UPLOAD_MINUTES: i64 = 60;

const AVATAR_BUCKET: &str = "avatars";
const MEDIA_BUCKET: &str = "media";

/// Allow-list rather than deny-list. A deny-list is a promise to keep up with
/// every dangerous type ever invented, which nobody wins.
const AVATAR_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/heic"];

const ATTACHMENT_TYPES: [&str; 24] = [
    // images
    "image/jpeg", "image/png", "image/webp", "image/gif", "image/heic",
    // video
    "video/mp4", "video/quicktime", "video/webm", "video/3gpp",
    // audio and voice notes
    "audio/mpeg", "audio/mp4", "audio/aac", "audio/ogg", "audio/opus", "audio/wav",
    // documents
    "application/pdf",
    "application/msword",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/vnd.ms-excel",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/vnd.ms-powerpoint",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "text/plain",
    "application/zip",
];

pub struct MediaService {
    services: Services,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPurpose {
    /// Public bucket, unencrypted — an avatar is shown to people you have not
    /// yet exchanged keys with, so it cannot be end-to-end encrypted.
    Avatar,
    /// Private bucket. The client encrypts before upload and puts the content
    /// key inside the message ciphertext, so storage holds only opaque bytes.
    Attachment,
}

pub struct UploadTicket {
    pub media_id: MediaId,
    pub upload: SignedUpload,
}

impl MediaService {
    pub fn new(services: Services) -> Self {
        Self { services }
    }

    pub async fn request_upload(
        &self,
        owner: UserId,
        purpose: MediaPurpose,
        mime_type: &str,
        byte_size: i64,
        width: Option<i32>,
        height: Option<i32>,
        duration_ms: Option<i32>,
    ) -> DomainResult<UploadTicket> {
        let storage = self
            .services
            .storage
            .as_ref()
            .ok_or_else(|| DomainError::infrastructure("media storage is not configured"))?;

        let (bucket, allowed, max_bytes, encrypted) = match purpose {
            MediaPurpose::Avatar => (
                AVATAR_BUCKET,
                AVATAR_TYPES.as_slice(),
                MAX_AVATAR_BYTES,
                false,
            ),
            MediaPurpose::Attachment => (
                MEDIA_BUCKET,
                ATTACHMENT_TYPES.as_slice(),
                MAX_ATTACHMENT_BYTES,
                true,
            ),
        };

        let normalised = mime_type.trim().to_lowercase();
        if !allowed.contains(&normalised.as_str()) {
            return Err(DomainError::validation(format!(
                "'{normalised}' is not an accepted file type"
            )));
        }

        if byte_size <= 0 {
            return Err(DomainError::validation("file size is required"));
        }
        if byte_size > max_bytes {
            return Err(DomainError::validation(format!(
                "file is too large — the limit is {} MB",
                max_bytes / (1024 * 1024)
            )));
        }

        // Storage is billed by the gigabyte and uploads are the cheapest thing
        // to abuse, since the bytes never touch us.
        self.services
            .rate_limiter
            .check(&format!("media:upload:{owner}"), 120, 3_600)
            .await?;

        // Path is opaque and unguessable. Deriving it from the filename would
        // let one user overwrite another's object, and would leak the name.
        let key = format!(
            "{}/{}.{}",
            owner,
            Uuid::now_v7(),
            extension_for(&normalised)
        );

        let asset = self
            .services
            .media
            .create_pending(NewMedia {
                owner_id: owner,
                bucket: bucket.to_string(),
                key: key.clone(),
                mime_type: normalised.clone(),
                byte_size,
                width,
                height,
                duration_ms,
                is_encrypted: encrypted,
            })
            .await?;

        let upload = storage.signed_upload(&key, &normalised).await?;

        Ok(UploadTicket {
            media_id: asset.id,
            upload,
        })
    }

    /// Called once the client's PUT has succeeded.
    pub async fn complete(
        &self,
        owner: UserId,
        media_id: MediaId,
        byte_size: i64,
    ) -> DomainResult<MediaAsset> {
        let completed = self
            .services
            .media
            .mark_complete(media_id, owner, byte_size)
            .await?;

        if !completed {
            return Err(DomainError::not_found("upload"));
        }

        self.services
            .media
            .find(media_id)
            .await?
            .ok_or_else(|| DomainError::not_found("media"))
    }

    /// A URL the client can read from. Avatars resolve to a public path;
    /// anything else gets a signed link that expires.
    pub async fn download_url(&self, media_id: MediaId) -> DomainResult<String> {
        let storage = self
            .services
            .storage
            .as_ref()
            .ok_or_else(|| DomainError::infrastructure("media storage is not configured"))?;

        let asset = self
            .services
            .media
            .find(media_id)
            .await?
            .ok_or_else(|| DomainError::not_found("media"))?;

        if !asset.is_complete() {
            return Err(DomainError::not_found("media"));
        }

        storage
            .download_url(&asset.key, asset.bucket == AVATAR_BUCKET)
            .await
    }

    /// Deletes uploads that were started and abandoned. Run periodically.
    pub async fn sweep_stale(&self) -> DomainResult<u64> {
        let Some(storage) = self.services.storage.as_ref() else {
            return Ok(0);
        };

        let stale = self
            .services
            .media
            .stale_pending(STALE_UPLOAD_MINUTES)
            .await?;

        let mut removed = 0;
        for asset in stale {
            // Storage first: a row without an object is harmless, an object
            // without a row is unreachable and unbilled to anyone.
            storage.delete(&asset.key).await.ok();
            self.services.media.delete(asset.id).await.ok();
            removed += 1;
        }

        Ok(removed)
    }
}

fn extension_for(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/heic" => "heic",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "video/3gpp" => "3gp",
        "audio/mpeg" => "mp3",
        "audio/mp4" | "audio/aac" => "m4a",
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/wav" => "wav",
        "application/pdf" => "pdf",
        "application/msword" => "doc",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.ms-excel" => "xls",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.ms-powerpoint" => "ppt",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => "pptx",
        "text/plain" => "txt",
        "application/zip" => "zip",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avatar_types_are_a_subset_of_attachment_types() {
        for kind in AVATAR_TYPES {
            assert!(
                ATTACHMENT_TYPES.contains(&kind),
                "{kind} accepted for avatars but not attachments"
            );
        }
    }

    #[test]
    fn every_accepted_type_maps_to_an_extension() {
        for kind in ATTACHMENT_TYPES {
            assert_ne!(extension_for(kind), "bin", "no extension mapped for {kind}");
        }
    }
}
