//! Route table and the middleware stack.

use std::time::Duration;

use axum::extract::Request;
use axum::http::{header, HeaderValue, Method};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{delete, get, patch, post, put};
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::routes::*;
use crate::state::ApiState;
use crate::ws;

/// Media never passes through this service — it goes to object storage via
/// presigned URLs — so a small body limit is correct. Anything larger is a bug
/// or an attack.
const MAX_BODY_BYTES: usize = 512 * 1024;

pub struct RouterConfig {
    /// Exact origins for the web client. Empty means "no browser origin",
    /// which is the right default for a mobile-only deployment.
    pub allowed_origins: Vec<String>,
    pub enable_docs: bool,
}

pub fn build_router(state: ApiState, config: RouterConfig) -> Router {
    // Reachable without a token.
    let public = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/v1/auth/request-otp", post(auth::request_otp))
        .route("/v1/auth/verify-otp", post(auth::verify_otp))
        .route("/v1/auth/refresh", post(auth::refresh))
        // Pairing: the browser has no session yet, so creating and polling a
        // code are unauthenticated. Approving is not — see below.
        .route("/v1/devices/link-requests", post(device_links::create))
        .route("/v1/devices/link-requests/:code", get(device_links::poll));

    // Everything below requires a valid access token. Authentication is
    // enforced by the `CurrentUser` extractor in each handler's signature, so
    // a handler cannot be written that forgets it.
    let authenticated = Router::new()
        .route("/v1/auth/logout", post(auth::logout))
        .route("/v1/me", get(users::me).patch(users::update_me))
        .route("/v1/me/privacy", get(users::privacy).patch(users::update_privacy))
        .route("/v1/me/blocks", post(users::block))
        .route("/v1/me/blocks/:user_id", delete(users::unblock))
        .route("/v1/me/devices", get(users::list_devices))
        .route("/v1/me/devices/push-token", post(users::register_push_token))
        .route("/v1/me/devices/:device_id", delete(users::revoke_device))
        .route("/v1/me/security-events", get(users::security_events))
        .route(
            "/v1/devices/link-requests/:code/approve",
            post(device_links::approve),
        )
        .route(
            "/v1/me/two-step",
            post(users::set_two_step_pin).delete(users::disable_two_step),
        )
        .route("/v1/me/two-step/verify", post(users::verify_two_step))
        .route("/v1/users/sync-contacts", post(users::sync_contacts))
        // Declared before `/:user_id` so the literal segment is not swallowed
        // by the parameterised route.
        .route("/v1/users/by-username/:username", get(users::get_by_username))
        .route("/v1/users/:user_id", get(users::get_user))
        // conversations
        .route("/v1/conversations", get(conversations::list))
        .route("/v1/conversations/direct", post(conversations::create_direct))
        .route("/v1/conversations/group", post(conversations::create_group))
        .route("/v1/conversations/:conversation_id", get(conversations::get))
        .route(
            "/v1/conversations/:conversation_id/members",
            post(conversations::add_members),
        )
        .route(
            "/v1/conversations/:conversation_id/members/:user_id",
            delete(conversations::remove_member),
        )
        .route(
            "/v1/conversations/:conversation_id/members/:user_id/role",
            put(conversations::set_role),
        )
        .route("/v1/conversations/:conversation_id/mute", post(conversations::mute))
        .route(
            "/v1/conversations/:conversation_id/delivered",
            post(conversations::mark_delivered),
        )
        .route("/v1/conversations/:conversation_id/read", post(conversations::mark_read))
        .route(
            "/v1/conversations/:conversation_id/messages",
            get(messages::list),
        )
        .route(
            "/v1/conversations/:conversation_id/notifications",
            get(notifications::get_conversation_settings)
                .patch(notifications::update_conversation_settings),
        )
        // messages
        // calls — signalling only; media goes to the SFU
        .route("/v1/calls", get(calls::history).post(calls::start))
        .route("/v1/calls/:call_id/join", post(calls::join))
        .route("/v1/calls/:call_id/end", post(calls::end))
        // media
        .route("/v1/media/uploads", post(media::request_upload))
        .route("/v1/media/:media_id", get(media::get))
        .route("/v1/media/:media_id/complete", post(media::complete))
        // messages
        .route("/v1/messages", post(messages::send))
        .route(
            "/v1/messages/:message_id",
            patch(messages::edit).delete(messages::delete),
        )
        .route("/v1/messages/:message_id/reactions", post(messages::react))
        // notifications
        .route("/v1/notifications/tones", get(notifications::list_tones))
        .route(
            "/v1/notifications/preferences",
            get(notifications::get_preferences).patch(notifications::update_preferences),
        )
        // encryption. `/count` is declared before `/:user_id` so it is not
        // swallowed by the parameterised route.
        .route("/v1/keys", post(keys::publish))
        .route("/v1/keys/count", get(keys::count))
        .route("/v1/keys/:user_id", get(keys::bundles));

    // The socket authenticates from its query string, since browsers cannot
    // set headers on a WebSocket handshake.
    let realtime = Router::new().route("/v1/ws", get(ws::handler));

    let mut router = Router::new().merge(public).merge(authenticated).merge(realtime);

    if config.enable_docs {
        router = router.merge(
            SwaggerUi::new("/docs").url("/docs/openapi.json", ApiDoc::openapi()),
        );
    }

    router
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(MAX_BODY_BYTES))
        .layer(TimeoutLayer::new(Duration::from_secs(20)))
        .layer(cors(&config.allowed_origins))
        // Security headers. Cheap, and they close off whole classes of attack
        // against the web client.
        .layer(SetResponseHeaderLayer::overriding(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains; preload"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        ))
        .layer(middleware::from_fn(content_security_policy))
        // This is an API: nothing it returns should ever sit in a shared cache.
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .with_state(state)
}

/// Content-Security-Policy, chosen per path.
///
/// The API itself gets `default-src 'none'` — it returns JSON and should never
/// be able to load a script, a font or an image. That is the right policy and
/// it stays.
///
/// Swagger UI cannot live under it. It is a real web page served from this same
/// origin, and it loads its own stylesheet, bundle and initialiser, so
/// `default-src 'none'` blocks every one of them and renders a blank page. It
/// also uses inline styles, which is why `'unsafe-inline'` appears for styles
/// only — never for scripts.
///
/// Scoping the relaxation to `/docs` keeps the weaker policy off every endpoint
/// that actually handles user data.
const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'";

const DOCS_CSP: &str = concat!(
    "default-src 'self'; ",
    "script-src 'self' 'unsafe-inline'; ",
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data:; ",
    "font-src 'self'; ",
    "connect-src 'self'; ",
    "frame-ancestors 'none'",
);

async fn content_security_policy(request: Request, next: Next) -> Response {
    let is_docs = request.uri().path().starts_with("/docs");

    let mut response = next.run(request).await;
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(if is_docs { DOCS_CSP } else { API_CSP }),
    );

    response
}

/// An explicit allow-list, never `permissive`. A wildcard here would let any
/// site on the internet make credentialed calls on behalf of a signed-in user.
fn cors(allowed_origins: &[String]) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(3_600))
}
