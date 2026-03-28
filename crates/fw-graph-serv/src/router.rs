//! Axum router — wires all handlers together with middleware.

use crate::{handler, state::AppState, ws};
use axum::{routing::get, Router};
use tower_http::{cors::CorsLayer, trace::TraceLayer};

/// Build the production [`Router`] with CORS, tracing, and all routes.
///
/// Routes:
/// - `POST /graphql`   — GraphQL HTTP endpoint
/// - `GET  /graphql`   — WebSocket upgrade (graphql-transport-ws)
/// - `GET  /playground` — GraphQL Playground UI
/// - `GET  /health`    — Liveness probe
pub fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    Router::new()
        .route(
            "/graphql",
            get(ws::ws_handler).post(handler::graphql_handler),
        )
        .route("/playground", get(handler::graphql_playground))
        .route("/health", get(health_check))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health_check() -> &'static str {
    "ok"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn health_check_returns_ok() {
        let response = health_check().await;
        assert_eq!(response, "ok");
    }
}
