use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use axum::extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
struct AppState {
    messages: Arc<RwLock<Vec<Message>>>,
    broadcaster: broadcast::Sender<Message>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Message {
    id: String,
    sender: String,
    content: String,
    created_at: String,
}

#[derive(Clone, Debug, Deserialize)]
struct CreateMessageRequest {
    sender: String,
    content: String,
}

#[derive(Clone, Debug, Serialize)]
struct ErrorResponse {
    ok: bool,
    error: String,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ErrorResponse {
                ok: false,
                error: self.message,
            }),
        )
            .into_response()
    }
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "status": "healthy",
        "service": "noicetom"
    }))
}

async fn root() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "noicetom",
        "service": "Rust API",
        "version": env!("CARGO_PKG_VERSION"),
        "routes": [
            "/",
            "/health",
            "/api/health",
            "/api/version",
            "/api/messages",
            "/ws"
        ]
    }))
}

async fn version() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "noicetom",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn list_messages(State(state): State<AppState>) -> Json<Vec<Message>> {
    let messages = state.messages.read().unwrap();
    Json(messages.clone())
}

async fn create_message(
    State(state): State<AppState>,
    Json(payload): Json<CreateMessageRequest>,
) -> Result<Json<Message>, AppError> {
    let sender = payload.sender.trim();
    let content = payload.content.trim();

    if sender.is_empty() {
        return Err(AppError {
            status: StatusCode::BAD_REQUEST,
            message: "Sender name cannot be empty.".to_string(),
        });
    }

    if content.is_empty() {
        return Err(AppError {
            status: StatusCode::BAD_REQUEST,
            message: "Message content cannot be empty.".to_string(),
        });
    }

    let message = Message {
        id: Uuid::new_v4().to_string(),
        sender: sender.to_string(),
        content: content.to_string(),
        created_at: Utc::now().to_rfc3339(),
    };

    {
        let mut messages = state.messages.write().unwrap();
        messages.push(message.clone());
    }

    // broadcast to websocket clients
    let _ = state.broadcaster.send(message.clone());

    Ok(Json(message))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    // send existing history first
    let history = { state.messages.read().unwrap().clone() };

    for m in history {
        if socket.send(WsMessage::Text(serde_json::to_string(&m).unwrap())).await.is_err() {
            return;
        }
    }

    let mut rx = state.broadcaster.subscribe();

    // Split the socket into sink and stream
    let (mut sender, mut receiver) = socket.split();

    // task to forward broadcast messages to this socket
    let mut rx2 = rx.resubscribe();
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx2.recv().await {
            let txt = serde_json::to_string(&msg).unwrap();
            if sender.send(WsMessage::Text(txt)).await.is_err() {
                break;
            }
        }
    });

    // receive messages from client
    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            WsMessage::Text(text) => {
                // try parse as CreateMessageRequest
                if let Ok(payload) = serde_json::from_str::<CreateMessageRequest>(&text) {
                    let message = Message {
                        id: Uuid::new_v4().to_string(),
                        sender: payload.sender.trim().to_string(),
                        content: payload.content.trim().to_string(),
                        created_at: Utc::now().to_rfc3339(),
                    };

                    {
                        let mut messages = state.messages.write().unwrap();
                        messages.push(message.clone());
                    }

                    // broadcast to all other clients
                    let _ = state.broadcaster.send(message);
                }
            }
            WsMessage::Close(_) => break,
            WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Binary(_) => {}
        }
    }

    // cleanup
    send_task.abort();
}

fn app() -> Router {
    let messages = Arc::new(RwLock::new(Vec::new()));
    let (tx, _rx) = broadcast::channel(100);

    let state = AppState {
        messages,
        broadcaster: tx,
    };

    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/messages", get(list_messages).post(create_message))
        .route("/ws", get(ws_handler))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl-c signal");

    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
        let _ = sigterm.recv().await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .with_target(false)
        .init();

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid integer");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("noicetom API listening on http://{}", addr);

    axum::serve(listener, app().into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{app, CreateMessageRequest};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_route_returns_ok() {
        let app = app();
        let request = Request::builder().uri("/api/health").body(Body::empty()).unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_message_route_persists_message() {
        let app = app();
        let payload = CreateMessageRequest {
            sender: "alice".to_string(),
            content: "hello from rust".to_string(),
        };

        let request = Request::builder()
            .method("POST")
            .uri("/api/messages")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&payload).unwrap()))
            .unwrap();

        let response = app.clone().oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        assert!(text.contains("hello from rust"));
        assert!(text.contains("alice"));
    }
}
