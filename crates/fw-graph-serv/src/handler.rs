//! GraphQL HTTP handler — creates per-request RLS connection and playground.

use crate::jwt::{decode_jwt, extract_bearer_token};
use crate::state::AppState;
use async_graphql::http::GraphQLPlaygroundConfig;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
};
use fw_graph_build::RequestConnection;

/// Main GraphQL POST handler.
///
/// Flow:
/// 1. Extract Authorization header, decode JWT.
/// 2. Acquire a connection from pool and apply RLS context → RequestConnection.
/// 3. Execute GraphQL request with both claims and RequestConnection as data.
pub async fn graphql_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok());
    let token = extract_bearer_token(auth_header);

    let claims = match decode_jwt(token, &state.jwt_secret, state.default_role.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            return async_graphql::Response::from_errors(vec![
                async_graphql::ServerError::new(e.to_string(), None),
            ])
            .into();
        }
    };

    // Acquire per-request connection with RLS applied.
    let req_conn = match RequestConnection::new(&state.pool, &claims).await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "failed to acquire RLS connection");
            return async_graphql::Response::from_errors(vec![
                async_graphql::ServerError::new("Database connection failed", None),
            ])
            .into();
        }
    };

    let schema_gen = state.schema_registry.current_schema().await;

    let inner_req = req.into_inner()
        .data(claims)
        .data(req_conn);

    schema_gen.schema.execute(inner_req).await.into()
}

/// Serve the GraphQL Playground UI at `GET /playground`.
pub async fn graphql_playground() -> impl IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        GraphQLPlaygroundConfig::new("/graphql"),
    ))
}
