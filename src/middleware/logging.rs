use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultOnResponse, TraceLayer};
use tracing::Level;

/// Logs every request/response: method, path, status, latency. Configured
/// to log at INFO (tower-http's default is DEBUG, invisible under this
/// app's default `RUST_LOG=info`).
pub fn logging_layer() -> TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    tower_http::trace::DefaultMakeSpan,
    tower_http::trace::DefaultOnRequest,
    DefaultOnResponse,
> {
    TraceLayer::new_for_http()
        .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO).latency_unit(tower_http::LatencyUnit::Millis))
}
