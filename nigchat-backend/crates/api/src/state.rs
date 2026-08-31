//! Shared HTTP state.
//!
//! Cheap to clone. Note what is *not* here: no session map, no membership
//! cache, no per-user counters. Any such field would silently make the process
//! stateful and break horizontal scaling.

use std::sync::Arc;

use nigchat_application::auth::AuthService;
use nigchat_application::conversations::ConversationService;
use nigchat_application::devices::DeviceService;
use nigchat_application::keys::KeyService;
use nigchat_application::messaging::MessagingService;
use nigchat_application::Services;

use crate::ws::Hub;

#[derive(Clone)]
pub struct ApiState {
    pub services: Services,
    pub auth: Arc<AuthService>,
    pub conversations: Arc<ConversationService>,
    pub messaging: Arc<MessagingService>,
    pub devices: Arc<DeviceService>,
    pub keys: Arc<KeyService>,
    /// Sockets owned by *this* process. Delivery to users connected elsewhere
    /// is the event bus's job — this deliberately knows nothing about other
    /// instances.
    pub hub: Arc<Hub>,
    /// Whether X-Forwarded-For may be believed. False unless the deployment
    /// actually sits behind a trusted proxy.
    pub trust_proxy_headers: bool,
}

impl ApiState {
    pub fn new(services: Services, debug_echo_codes: bool, trust_proxy_headers: bool) -> Self {
        Self {
            auth: Arc::new(AuthService::new(services.clone(), debug_echo_codes)),
            conversations: Arc::new(ConversationService::new(services.clone())),
            messaging: Arc::new(MessagingService::new(services.clone())),
            devices: Arc::new(DeviceService::new(services.clone())),
            keys: Arc::new(KeyService::new(services.clone())),
            hub: Arc::new(Hub::new()),
            trust_proxy_headers,
            services,
        }
    }
}
