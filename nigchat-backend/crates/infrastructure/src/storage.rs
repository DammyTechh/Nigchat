//! Supabase Storage.
//!
//! Chosen over raw S3 because you already have a Supabase project, and its
//! signed-upload endpoint is a single authenticated POST — whereas S3 presigning
//! means implementing SigV4 or pulling in the AWS SDK. Same result, far less to
//! go wrong.
//!
//! Two buckets, deliberately:
//!
//!   `avatars`  public. A profile photo is shown to people you have not yet
//!              exchanged keys with, so it cannot be end-to-end encrypted and
//!              there is nothing to gain from signing every read.
//!   `media`    private. Conversation attachments are encrypted on the device
//!              before upload, so storage holds opaque bytes, and reads are
//!              signed and expiring.

use async_trait::async_trait;
use nigchat_domain::ports::{ObjectStorage, SignedUpload};
use nigchat_domain::{DomainError, DomainResult};
use serde::Deserialize;

/// Supabase signs uploads for two hours. Long enough for a large file on a
/// poor connection, short enough that a leaked URL is not a standing invitation.
const UPLOAD_TTL_SECONDS: i64 = 7_200;

/// Signed reads. Short — the client fetches a fresh one when it needs it.
const DOWNLOAD_TTL_SECONDS: i64 = 3_600;

pub struct SupabaseStorage {
    client: reqwest::Client,
    /// e.g. https://abcdefg.supabase.co
    project_url: String,
    /// The **service role** key. It bypasses row-level security, so it must
    /// never reach a client — this is why signing happens on the server and the
    /// browser only ever sees the resulting URL.
    service_key: String,
    bucket: String,
}

impl SupabaseStorage {
    pub fn new(project_url: String, service_key: String, bucket: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .expect("failed to build HTTP client"),
            project_url: project_url.trim_end_matches('/').to_string(),
            service_key,
            bucket,
        }
    }
}

#[derive(Deserialize)]
struct SignedUploadResponse {
    /// Relative, e.g. `/object/upload/sign/media/abc.jpg?token=…`
    url: String,
}

#[derive(Deserialize)]
struct SignedDownloadResponse {
    #[serde(rename = "signedURL")]
    signed_url: String,
}

#[async_trait]
impl ObjectStorage for SupabaseStorage {
    async fn signed_upload(&self, key: &str, content_type: &str) -> DomainResult<SignedUpload> {
        let endpoint = format!(
            "{}/storage/v1/object/upload/sign/{}/{}",
            self.project_url, self.bucket, key
        );

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.service_key)
            .json(&serde_json::json!({ "expiresIn": UPLOAD_TTL_SECONDS }))
            .send()
            .await
            .map_err(|err| {
                tracing::error!(?err, "storage: signed upload request failed");
                DomainError::infrastructure("could not prepare the upload")
            })?;

        if !response.status().is_success() {
            // Status only — the body can echo the path and the token.
            tracing::error!(status = %response.status(), "storage rejected the sign request");
            return Err(DomainError::infrastructure("could not prepare the upload"));
        }

        let signed: SignedUploadResponse = response.json().await.map_err(|err| {
            tracing::error!(?err, "storage: unreadable sign response");
            DomainError::infrastructure("could not prepare the upload")
        })?;

        Ok(SignedUpload {
            url: format!(
                "{}/storage/v1/{}",
                self.project_url,
                signed.url.trim_start_matches('/')
            ),
            method: "PUT".to_string(),
            headers: vec![
                ("content-type".to_string(), content_type.to_string()),
                // Refuse to silently overwrite. Keys are unguessable UUIDs, so a
                // collision means a bug, and a bug that destroys someone's photo
                // should fail loudly.
                ("x-upsert".to_string(), "false".to_string()),
            ],
            expires_in_seconds: UPLOAD_TTL_SECONDS,
        })
    }

    async fn download_url(&self, key: &str, public: bool) -> DomainResult<String> {
        if public {
            // No round trip and no expiry — correct for avatars.
            return Ok(format!(
                "{}/storage/v1/object/public/{}/{}",
                self.project_url, self.bucket, key
            ));
        }

        let endpoint = format!(
            "{}/storage/v1/object/sign/{}/{}",
            self.project_url, self.bucket, key
        );

        let response = self
            .client
            .post(&endpoint)
            .bearer_auth(&self.service_key)
            .json(&serde_json::json!({ "expiresIn": DOWNLOAD_TTL_SECONDS }))
            .send()
            .await
            .map_err(|err| {
                tracing::error!(?err, "storage: signed download request failed");
                DomainError::infrastructure("could not prepare the download")
            })?;

        if !response.status().is_success() {
            tracing::error!(status = %response.status(), "storage rejected the download sign");
            return Err(DomainError::infrastructure("could not prepare the download"));
        }

        let signed: SignedDownloadResponse = response.json().await.map_err(|err| {
            tracing::error!(?err, "storage: unreadable download response");
            DomainError::infrastructure("could not prepare the download")
        })?;

        Ok(format!(
            "{}/storage/v1{}",
            self.project_url,
            if signed.signed_url.starts_with('/') {
                signed.signed_url
            } else {
                format!("/{}", signed.signed_url)
            }
        ))
    }

    async fn delete(&self, key: &str) -> DomainResult<()> {
        let endpoint = format!(
            "{}/storage/v1/object/{}/{}",
            self.project_url, self.bucket, key
        );

        self.client
            .delete(&endpoint)
            .bearer_auth(&self.service_key)
            .send()
            .await
            .map_err(|err| {
                tracing::warn!(?err, "storage: delete failed");
                DomainError::infrastructure("could not delete the object")
            })?;

        Ok(())
    }
}
