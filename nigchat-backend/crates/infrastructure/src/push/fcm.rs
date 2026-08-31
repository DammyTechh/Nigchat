//! Firebase Cloud Messaging (HTTP v1).
//!
//! Android and web. The tone travels as a data field rather than a `sound`
//! key, because Android 8+ binds sound to the notification *channel*, not the
//! message — the client selects the channel from `tone_id`.

use async_trait::async_trait;
use nigchat_domain::entities::PushProvider;
use nigchat_domain::ports::{PushMessage, PushOutcome, PushSender};
use nigchat_domain::values::PreviewMode;
use nigchat_domain::DomainResult;

pub struct FcmSender {
    client: reqwest::Client,
    project_id: String,
    /// OAuth2 bearer for the service account. Refreshed out of band; a
    /// production deployment should swap this for a token source that renews
    /// automatically before expiry.
    access_token: String,
}

impl FcmSender {
    pub fn new(project_id: String, access_token: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
            project_id,
            access_token,
        }
    }
}

#[async_trait]
impl PushSender for FcmSender {
    fn provider(&self) -> PushProvider {
        PushProvider::Fcm
    }

    async fn send(&self, message: PushMessage) -> DomainResult<PushOutcome> {
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );

        // With previews hidden the payload is data-only: no notification block
        // at all, so the OS cannot render anything the user asked to keep
        // private. The client wakes and decides what to show.
        let data_only = message.plan.preview_mode == PreviewMode::Hidden;

        let mut payload = serde_json::json!({
            "message": {
                "token": message.token,
                "data": {
                    "payload": message.data.to_string(),
                    "tone_id": message.plan.tone_id.clone().unwrap_or_default(),
                    "category": message.plan.category.as_str(),
                    "deep_link": message.deep_link.clone().unwrap_or_default(),
                },
                "android": {
                    "priority": if message.plan.high_priority { "HIGH" } else { "NORMAL" },
                    // Collapsing replaces an earlier notification from the same
                    // conversation instead of stacking a dozen of them.
                    "collapse_key": message.plan.collapse_key.clone().unwrap_or_default(),
                    "notification": {
                        // The channel carries the sound on modern Android.
                        "channel_id": message.plan.tone_id.clone().unwrap_or_else(|| "tone.message.default".into()),
                        "notification_priority": if message.plan.high_priority { "PRIORITY_MAX" } else { "PRIORITY_DEFAULT" },
                        "default_vibrate_timings": message.plan.vibration != nigchat_domain::values::Vibration::Off,
                    }
                }
            }
        });

        if !data_only {
            payload["message"]["notification"] = serde_json::json!({
                "title": message.title,
                "body": message.body,
            });
        }

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&payload)
            .send()
            .await;

        let response = match response {
            Ok(response) => response,
            Err(err) => return Ok(PushOutcome::Retryable(err.to_string())),
        };

        let status = response.status();
        if status.is_success() {
            let id = response
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|body| body.get("name")?.as_str().map(str::to_string));
            return Ok(PushOutcome::Delivered {
                provider_message_id: id,
            });
        }

        // 404 UNREGISTERED and 400 INVALID_ARGUMENT on the token mean the
        // registration is dead: retire it rather than paying to retry.
        if status == reqwest::StatusCode::NOT_FOUND || status == reqwest::StatusCode::BAD_REQUEST {
            return Ok(PushOutcome::TokenInvalid);
        }
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(PushOutcome::Retryable(format!("fcm status {status}")));
        }

        Ok(PushOutcome::Failed(format!("fcm status {status}")))
    }
}
