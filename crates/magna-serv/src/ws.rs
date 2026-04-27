//! WebSocket handler for the `graphql-transport-ws` protocol.

use crate::state::AppState;
use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};

/// The standard protocol identifier for the modern GraphQL over WebSocket spec.
pub const GRAPHQL_TRANSPORT_WS_PROTOCOL: &str = "graphql-transport-ws";

/// The legacy Apollo subscription protocol — rejected with 400.
const GRAPHQL_WS_PROTOCOL: &str = "graphql-ws";

/// WebSocket upgrade handler mounted at `GET /graphql`.
///
/// Rejects legacy `graphql-ws` protocol clients with HTTP 400.
/// All other connections are upgraded and handed to
/// [`handle_ws_connection`] using the `graphql-transport-ws` sub-protocol.
pub async fn ws_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Inspect the Sec-WebSocket-Protocol header.
    let requested_protocol = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if requested_protocol
        .split(',')
        .any(|p| p.trim() == GRAPHQL_WS_PROTOCOL)
    {
        // Reject legacy graphql-ws clients.
        return (
            StatusCode::BAD_REQUEST,
            "The graphql-ws protocol is not supported. Please use graphql-transport-ws.",
        )
            .into_response();
    }

    ws.protocols([GRAPHQL_TRANSPORT_WS_PROTOCOL])
        .on_upgrade(move |socket| handle_ws_connection(socket, state))
        .into_response()
}

/// Drive a single WebSocket connection through the `graphql-transport-ws` handshake.
///
/// Supported messages:
/// - `connection_init`  → send `connection_ack`
/// - `subscribe`        → send `complete` (stub — full streaming not yet implemented)
/// - `complete`         → acknowledged silently
/// - anything else      → ignored
pub async fn handle_ws_connection(socket: WebSocket, _state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    while let Some(msg_result) = receiver.next().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(_) => break,
        };

        let text = match msg {
            Message::Text(t) => t,
            Message::Close(_) => break,
            _ => continue,
        };

        // Parse the message type from JSON.
        let parsed: serde_json::Value = match serde_json::from_str(text.as_str()) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = parsed
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        match msg_type {
            "connection_init" => {
                let ack = serde_json::json!({ "type": "connection_ack" }).to_string();
                if sender.send(Message::Text(ack.into())).await.is_err() {
                    break;
                }
            }
            "subscribe" => {
                // Extract the subscription id so we can send back `complete`.
                let id = parsed
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned();

                let complete = serde_json::json!({ "type": "complete", "id": id }).to_string();
                if sender.send(Message::Text(complete.into())).await.is_err() {
                    break;
                }
            }
            "complete" => {
                // Client signals it is done with a subscription — nothing to do.
            }
            _ => {
                // Unknown message type — ignore.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_constant_value() {
        assert_eq!(GRAPHQL_TRANSPORT_WS_PROTOCOL, "graphql-transport-ws");
    }
}
