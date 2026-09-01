//! SMS delivery for verification codes.
//!
//! Behind a trait so the provider is a deployment decision. Neither
//! implementation ever logs the code or the full number.

use async_trait::async_trait;
use nigchat_domain::ports::SmsSender;
use nigchat_domain::values::PhoneNumber;
use nigchat_domain::{DomainError, DomainResult};

/// Development sender. Logs that a code was issued, never the code itself —
/// the code is returned in the API response when `OTP_DEBUG_ECHO` is on, and
/// `server` refuses to boot with that outside development.
pub struct LoggingSmsSender;

#[async_trait]
impl SmsSender for LoggingSmsSender {
    async fn send_verification_code(&self, phone: &PhoneNumber, _code: &str) -> DomainResult<()> {
        tracing::info!(phone = %phone, "verification SMS suppressed (development sender)");
        Ok(())
    }
}

/// Generic HTTP provider. Works with Termii, Africa's Talking, Twilio and
/// similar by templating the request body.
pub struct HttpSmsSender {
    client: reqwest::Client,
    endpoint: String,
    api_key: String,
    sender_id: String,
}

impl HttpSmsSender {
    pub fn new(endpoint: String, api_key: String, sender_id: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                // A hung SMS provider must not hold an OTP request open.
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("failed to build HTTP client"),
            endpoint,
            api_key,
            sender_id,
        }
    }
}

#[async_trait]
impl SmsSender for HttpSmsSender {
    async fn send_verification_code(&self, phone: &PhoneNumber, code: &str) -> DomainResult<()> {
        // Termii v4. Two fields decide whether this arrives at all:
        //
        //   channel: "dnd"  Most Nigerian numbers sit on the do-not-disturb
        //                   list. The "generic" channel is silently dropped for
        //                   them, which looks exactly like a code that never
        //                   came. DND routes cost more and deliver.
        //   from            Must be an approved sender ID. Approval takes a day
        //                   or two; until then Termii's generic sender works.
        let body = serde_json::json!({
            "to": phone.as_str(),
            "from": self.sender_id,
            "sms": format!("{code} is your NigChat verification code. It expires in 5 minutes. Never share it."),
            "type": "plain",
            "channel": "dnd",
            "api_key": self.api_key,
        });

        let response = self
            .client
            .post(&self.endpoint)
            .json(&body)
            .send()
            .await
            .map_err(|err| {
                tracing::error!(?err, "SMS provider request failed");
                DomainError::infrastructure("could not send verification code")
            })?;

        let status = response.status();

        if !status.is_success() {
            // Status only, never the body: Termii echoes the message back, and
            // the message contains the verification code.
            tracing::error!(%status, "SMS provider rejected the request");
            return Err(DomainError::infrastructure(
                "could not send verification code",
            ));
        }

        // Termii answers 200 even when it refuses the message — an unapproved
        // sender ID, an empty wallet, an unroutable number. The outcome is in
        // the body, so a 200 alone is not proof of anything.
        if let Ok(payload) = response.json::<serde_json::Value>().await {
            let code_field = payload.get("code").and_then(|v| v.as_str());
            if let Some(reason) = code_field.filter(|c| *c != "ok") {
                tracing::error!(reason, "SMS provider accepted the request but did not send");
                return Err(DomainError::infrastructure(
                    "could not send verification code",
                ));
            }
        }

        tracing::info!(phone = %phone, "verification SMS dispatched");
        Ok(())
    }
}
