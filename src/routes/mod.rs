pub mod health;
pub mod v1;

use crate::{
    auth::client_ip, config::Env, error::Result, middleware::logging_layer, state::AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Request, State},
    http::{
        HeaderName, HeaderValue, Method,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    middleware::Next,
    response::Response,
    routing::{Router, get},
};
use std::time::Duration;
use tower_http::{
    compression::CompressionLayer, cors::CorsLayer, set_header::SetResponseHeaderLayer,
    timeout::TimeoutLayer,
};

/// A backstop across every route, including ones added later. Per-endpoint
/// limits do the precise work; this exists so an unauthenticated flood at a
/// route nobody remembered to protect still costs the caller something.
///
/// Generous on purpose: mobile carriers put thousands of people behind one
/// address, so this should only catch a single address behaving like a
/// script, never a shared network behaving like a city.
const GLOBAL_REQUESTS_PER_HOUR: u32 = 3000;

/// Longer than any outbound call this service makes, so a request can only
/// time out here once its own dependencies already have. Without it a hung
/// connection holds a worker and a database handle indefinitely, which is
/// how a slow-loris takes a service down without any volume at all.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

pub fn create_router(state: AppState) -> Router {
    let config = state.config.clone();

    Router::new()
        .route("/", get(|| async { "TrueLabel API v0.1.0" }))
        .route("/health", get(health::health))
        .route("/health/live", get(health::liveness))
        .route("/health/ready", get(health::readiness))
        .nest("/api/v1", v1::v1_router())
        .layer(axum::middleware::from_fn_with_state(state.clone(), guard))
        .with_state(state)
        // A nutrition label is a few hundred bytes; the framework default is
        // 2 MB of our memory per concurrent request.
        .layer(DefaultBodyLimit::max(256 * 1024))
        // Set on every response rather than per route, so a route added
        // tomorrow is covered without anybody remembering to cover it.
        //
        // `no-store` matters most: an authenticated reply carries a profile
        // and a subscription state, and without it a shared proxy is free to
        // keep a copy and hand it to the next person through.
        .layer(header("cache-control", "no-store"))
        .layer(header("x-content-type-options", "nosniff"))
        .layer(header("x-frame-options", "DENY"))
        .layer(header(
            "content-security-policy",
            "default-src 'none'; frame-ancestors 'none'",
        ))
        .layer(header("referrer-policy", "no-referrer"))
        .layer(header(
            "strict-transport-security",
            "max-age=63072000; includeSubDomains",
        ))
        .layer(CompressionLayer::new())
        .layer(cors(&config))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(logging_layer())
}

fn header(name: &'static str, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::overriding(
        HeaderName::from_static(name),
        HeaderValue::from_static(value),
    )
}

async fn guard(State(state): State<AppState>, request: Request, next: Next) -> Result<Response> {
    // An orchestrator polls these. Limiting them would pull the service out
    // of rotation under exactly the load the limit exists to survive.
    if request.uri().path().starts_with("/health") {
        return Ok(next.run(request).await);
    }

    let (parts, body) = request.into_parts();
    let ip = client_ip(&parts, state.config.trusted_proxy_hops);
    state
        .limit("global", &ip, GLOBAL_REQUESTS_PER_HOUR, 3600)
        .await?;
    Ok(next.run(Request::from_parts(parts, body)).await)
}

/// Browsers only. A native client sends no `Origin`, so an empty allowlist —
/// the default — costs the app nothing and stops the API being used as
/// somebody else's free backend.
///
/// Credentials are never allowed and never needed: this API authenticates
/// with an `Authorization` header, which a browser never attaches on its own.
/// That is what makes CSRF structurally impossible here rather than merely
/// mitigated, and it is worth keeping that way — the moment a cookie carries
/// auth, every state-changing route needs a token of its own.
fn cors(config: &Env) -> CorsLayer {
    let origins: Vec<HeaderValue> = config
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    if origins.is_empty() {
        return CorsLayer::new();
    }

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
        .max_age(Duration::from_secs(3600))
}
