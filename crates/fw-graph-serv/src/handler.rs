//! GraphQL HTTP handler and playground endpoint.

use crate::jwt::{decode_jwt, extract_bearer_token};
use crate::state::AppState;
use async_graphql::http::GraphQLPlaygroundConfig;
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    extract::State,
    http::HeaderMap,
    response::IntoResponse,
};

/// Main GraphQL POST handler.
///
/// Flow:
/// 1. Extract the `Authorization` header and strip the `Bearer ` prefix.
/// 2. Decode and validate the JWT (unauthenticated requests get anon claims).
/// 3. Fetch the current schema generation from the registry.
/// 4. Execute the GraphQL request with the decoded claims as data.
pub async fn graphql_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: GraphQLRequest,
) -> GraphQLResponse {
    // --- 1. Extract bearer token ---
    let auth_header = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok());
    let token = extract_bearer_token(auth_header);

    // --- 2. Decode JWT ---
    let claims = match decode_jwt(token, &state.jwt_secret, state.default_role.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            return async_graphql::Response::from_errors(vec![
                async_graphql::ServerError::new(e.to_string(), None),
            ])
            .into();
        }
    };

    // --- 3. Get current schema ---
    let schema_gen = state.schema_registry.current_schema().await;

    // --- 4. Execute with claims as data ---
    let inner_req = req.into_inner().data(claims);
    schema_gen.schema.execute(inner_req).await.into()
}

/// Serve the GraphQL Playground UI at `GET /playground`.
pub async fn graphql_playground() -> impl IntoResponse {
    axum::response::Html(async_graphql::http::playground_source(
        GraphQLPlaygroundConfig::new("/graphql"),
    ))
}
