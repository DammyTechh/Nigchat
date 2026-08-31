//! Apple Push Notification service (HTTP/2).
//!
//! iOS, iPadOS and macOS. Two behaviours here are Apple-specific and easy to
//! get wrong:
//!
//! * `apns-push-type` must match the payload. A VoIP push that is not a real
//!   incoming call is an entitlement violation, and Apple will revoke it.
//! * A hidden preview must be sent as `mutable-content` with no alert body,
//!   so the notification service extension decrypts and renders locally.

use async_trait::async_trait;
use nigchat_domain::entities::PushProvider;
use nigchat_domain::ports::{PushMessage, PushOutcome, PushSender};
use nigchat_domain::values::PreviewMode;
use nigchat_domain::DomainResult;

pub struct ApnsSender {
    client: reqwest::Client,
    topic: String,
    /// JWT signed with the APNs auth key (ES256). Apple requires it to be
    /// refreshed at least hourly.
    auth_token: String,
}

impl ApnsSender {
    pub fn new(topic: String, auth_token: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                // APNs requires HTTP/2 and will reject a 1.1 handshake.
                .http2_prior_knowledge()
                .build()
                .expect("failed to build HTTP/2 client"),
            topic,
            auth_token,
        }
    }

    fn host(sandbox: bool) -> &'static str {
        if sandbox {
            "https://api.sandbox.push.apple.com"
        } else {
            "https://api.push.apple.com"
        }
    }
}

#[async_trait]
impl PushSender for ApnsSender {
    fn provider(&self) -> PushProvider {
        PushProvider::Apns
    }

    async fn send(&self, message: PushMessage) -> DomainResult<PushOutcome> {
        let url = format!(
            "{}/3/device/{}",
            Self::host(message.sandbox),
            message.token
        );

        let hidden = message.plan.preview_mode == PreviewMode::Hidden;

        // Sound must be the file name the app bundles. `tone_id` maps to it
        // through the tone catalogue; the client ships the audio.
        let sound = message
            .plan
            .tone_id
            .as_deref()
            .map(|tone| format!("{tone}.caf"))
            .unwrap_or_else(|| "default".to_string());

        let mut aps = serde_json::json!({
            "sound": sound,
            // Lets the notification service extension decrypt the ciphertext
            // and replace the placeholder before anything is shown.
            "mutable-content": 1,
            "thread-id": message.plan.collapse_key.clone().unwrap_or_default(),
        });

        if hidden {
            aps["alert"] = serde_json::json!({ "title": "NigChat", "body": "New message" });
        } else {
            aps["alert"] = serde_json::json!({ "title": message.title, "body": message.body });
        }
        if let Some(badge) = message.badge {
            aps["badge"] = serde_json::json!(badge);
        }

        let payload = serde_json::json!({
            "aps": aps,
            "nigchat": message.data,
            "deep_link": message.deep_link,
            "tone_id": message.plan.tone_id,
        });

        let push_type = if message.is_voip { "voip" } else { "alert" };
        let priority = if message.plan.high_priority { "10" } else { "5" };

        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.auth_token)
            .header("apns-topic", &self.topic)
            .header("apns-push-type", push_type)
            .header("apns-priority", priority)
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
                .headers()
                .get("apns-id")
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            return Ok(PushOutcome::Delivered {
                provider_message_id: id,
            });
        }

        // 410 Gone is Apple's explicit "this token is dead".
        if status == reqwest::StatusCode::GONE {
            return Ok(PushOutcome::TokenInvalid);
        }
        if status == reqwest::StatusCode::BAD_REQUEST {
            let reason = response.text().await.unwrap_or_default();
            if reason.contains("BadDeviceToken") || reason.contains("DeviceTokenNotForTopic") {
                return Ok(PushOutcome::TokenInvalid);
            }
            return Ok(PushOutcome::Failed(reason));
        }
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Ok(PushOutcome::Retryable(format!("apns status {status}")));
        }

        Ok(PushOutcome::Failed(format!("apns status {status}")))
    }
}
