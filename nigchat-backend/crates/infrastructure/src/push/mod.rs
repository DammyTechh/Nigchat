//! Push transport adapters.
//!
//! These carry out a decision that has already been made. Every policy
//! question — mute, quiet hours, tone, preview — was answered by
//! `domain::notifications::NotificationPolicy` before a sender is called.
//! Nothing here may decide *whether* to notify.

mod apns;
mod fcm;

pub use apns::ApnsSender;
pub use fcm::FcmSender;

use async_trait::async_trait;
use nigchat_domain::entities::PushProvider;
use nigchat_domain::ports::{PushMessage, PushOutcome, PushSender};
use nigchat_domain::DomainResult;

/// Used when a deployment has no credentials for a provider. Dispatch then
/// records "no valid token" instead of failing a message send — push must
/// never be able to break messaging.
pub struct NoopPushSender {
    provider: PushProvider,
}

impl NoopPushSender {
    pub fn new(provider: PushProvider) -> Self {
        Self { provider }
    }
}

#[async_trait]
impl PushSender for NoopPushSender {
    fn provider(&self) -> PushProvider {
        self.provider
    }

    async fn send(&self, message: PushMessage) -> DomainResult<PushOutcome> {
        tracing::debug!(
            provider = self.provider.as_str(),
            category = message.plan.category.as_str(),
            tone = ?message.plan.tone_id,
            "push suppressed: provider not configured"
        );
        Ok(PushOutcome::Failed("provider not configured".into()))
    }
}
