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

#[derive(Clone, Debug)]
struct AppState {
    messages: Arc<RwLock<Vec<Message>>>,
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
            "/api/messages"
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

    let mut messages = state.messages.write().unwrap();
    messages.push(message.clone());

    Ok(Json(message))
}

fn app() -> Router {
    let state = AppState {
        messages: Arc::new(RwLock::new(Vec::new())),
    };

    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/messages", get(list_messages).post(create_message))
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
