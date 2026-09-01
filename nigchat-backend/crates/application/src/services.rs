//! The dependency container.
//!
//! Every field is a trait object, so the application layer never names a
//! concrete adapter. `server` builds this once at boot and hands out clones —
//! it is the only place in the codebase that knows PostgreSQL and Redis exist.

use std::sync::Arc;

use nigchat_domain::ports::*;

#[derive(Clone)]
pub struct Services {
    // repositories
    pub users: Arc<dyn UserRepository>,
    pub devices: Arc<dyn DeviceRepository>,
    pub sessions: Arc<dyn SessionRepository>,
    pub challenges: Arc<dyn AuthChallengeRepository>,
    pub keys: Arc<dyn KeyRepository>,
    pub conversations: Arc<dyn ConversationRepository>,
    pub messages: Arc<dyn MessageRepository>,
    pub notifications: Arc<dyn NotificationRepository>,
    pub security: Arc<dyn SecurityRepository>,
    pub device_links: Arc<dyn DeviceLinkRepository>,

    // services
    pub clock: Arc<dyn Clock>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub events: Arc<dyn EventPublisher>,
    pub presence: Arc<dyn PresenceRegistry>,
    pub sms: Arc<dyn SmsSender>,
    pub hasher: Arc<dyn Hasher>,
    pub tokens: Arc<dyn TokenService>,
    /// One per provider. Empty in tests and in deployments without push
    /// credentials — dispatch degrades to "suppressed, no valid token" rather
    /// than failing a send.
    pub push: Vec<Arc<dyn PushSender>>,
}

impl Services {
    pub fn push_for(
        &self,
        provider: nigchat_domain::entities::PushProvider,
    ) -> Option<&Arc<dyn PushSender>> {
        self.push.iter().find(|sender| sender.provider() == provider)
    }
}
