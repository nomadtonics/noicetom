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
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

use noicetom::{Message, Storage};

#[derive(Clone, Debug)]
struct AppState {
    messages: Arc<RwLock<Vec<Message>>>,
    storage: Storage,
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
        "service": "noicetom-server"
    }))
}

async fn list_messages(State(state): State<AppState>) -> Result<Json<Vec<Message>>, AppError> {
    let messages = state.messages.read().unwrap().clone();
    Ok(Json(messages))
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

    if let Err(err) = state.storage.append_message(&message) {
        tracing::error!("failed to persist message: {}", err);
    }

    Ok(Json(message))
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/messages", get(list_messages).post(create_message))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to listen for ctrl_c signal");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let storage = Storage::new()?;
    let messages = storage.load_messages().unwrap_or_default();

    let state = AppState {
        messages: Arc::new(RwLock::new(messages)),
        storage,
    };

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid integer");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("noicetom server listening on http://{}", addr);

    axum::serve(listener, app(state).into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
