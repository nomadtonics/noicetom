use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
};
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use uuid::Uuid;

#[derive(Clone, Debug)]
struct AppState {
    messages: Arc<RwLock<Vec<Message>>>,
    broadcaster: broadcast::Sender<Message>,
    pool: Option<PgPool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
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

async fn list_messages(State(state): State<AppState>) -> Result<Json<Vec<Message>>, AppError> {
    if let Some(pool) = &state.pool {
        let messages = sqlx::query_as::<_, Message>(
            "SELECT id, sender, content, created_at FROM messages ORDER BY created_at ASC",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| AppError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Failed to read messages: {}", e),
        })?;
        return Ok(Json(messages));
    }

    let messages = state.messages.read().unwrap();
    Ok(Json(messages.clone()))
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

    let created_at = Utc::now().to_rfc3339();

    if let Some(pool) = &state.pool {
        let message = sqlx::query_as::<_, Message>(
            r#"
            INSERT INTO messages (id, sender, content, created_at)
            VALUES ($1, $2, $3, $4)
            RETURNING id, sender, content, created_at
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(sender.to_string())
        .bind(content.to_string())
        .bind(created_at)
        .fetch_one(pool)
        .await
        .map_err(|e| AppError {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("Failed to save message: {}", e),
        })?;

        let _ = state.broadcaster.send(message.clone());
        return Ok(Json(message));
    }

    let message = Message {
        id: Uuid::new_v4().to_string(),
        sender: sender.to_string(),
        content: content.to_string(),
        created_at,
    };

    {
        let mut messages = state.messages.write().unwrap();
        messages.push(message.clone());
    }

    let _ = state.broadcaster.send(message.clone());
    Ok(Json(message))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let history = if let Some(pool) = &state.pool {
        match sqlx::query_as::<_, Message>(
            "SELECT id, sender, content, created_at FROM messages ORDER BY created_at ASC",
        )
        .fetch_all(pool)
        .await
        {
            Ok(messages) => messages,
            Err(_) => Vec::new(),
        }
    } else {
        state.messages.read().unwrap().clone()
    };

    for message in history {
        if socket
            .send(WsMessage::Text(serde_json::to_string(&message).unwrap()))
            .await
            .is_err()
        {
            return;
        }
    }

    let mut rx = state.broadcaster.subscribe();
    let (mut sender, mut receiver) = socket.split();
    let mut rx2 = rx.resubscribe();
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx2.recv().await {
            let json = serde_json::to_string(&msg).unwrap();
            if sender.send(WsMessage::Text(json)).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            WsMessage::Text(text) => {
                if let Ok(payload) = serde_json::from_str::<CreateMessageRequest>(&text) {
                    let sender_name = payload.sender.trim().to_string();
                    let content = payload.content.trim().to_string();

                    if sender_name.is_empty() || content.is_empty() {
                        continue;
                    }

                    let message = if let Some(pool) = &state.pool {
                        let created_at = Utc::now().to_rfc3339();
                        match sqlx::query_as::<_, Message>(
                            r#"
                            INSERT INTO messages (id, sender, content, created_at)
                            VALUES ($1, $2, $3, $4)
                            RETURNING id, sender, content, created_at
                            "#,
                        )
                        .bind(Uuid::new_v4().to_string())
                        .bind(sender_name.clone())
                        .bind(content.clone())
                        .bind(created_at)
                        .fetch_one(pool)
                        .await
                        {
                            Ok(message) => message,
                            Err(_) => Message {
                                id: Uuid::new_v4().to_string(),
                                sender: sender_name,
                                content,
                                created_at,
                            },
                        }
                    } else {
                        let message = Message {
                            id: Uuid::new_v4().to_string(),
                            sender: sender_name,
                            content,
                            created_at: Utc::now().to_rfc3339(),
                        };
                        {
                            let mut messages = state.messages.write().unwrap();
                            messages.push(message.clone());
                        }
                        message
                    };

                    let _ = state.broadcaster.send(message);
                }
            }
            WsMessage::Close(_) => break,
            WsMessage::Ping(_) | WsMessage::Pong(_) | WsMessage::Binary(_) => {}
        }
    }

    send_task.abort();
}

fn app() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/messages", get(list_messages).post(create_message))
        .route("/ws", get(ws_handler))
        .with_state(AppState {
            messages: Arc::new(RwLock::new(Vec::new())),
            broadcaster: broadcast::channel(100).0,
            pool: None,
        }))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}

async fn app_with_db(pool: PgPool) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/api/health", get(health))
        .route("/api/version", get(version))
        .route("/api/messages", get(list_messages).post(create_message))
        .route("/ws", get(ws_handler))
        .with_state(AppState {
            messages: Arc::new(RwLock::new(Vec::new())),
            broadcaster: broadcast::channel(100).0,
            pool: Some(pool),
        }))
        .layer(CorsLayer::permissive())
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

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(
            &std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://postgres:postgres@db:5432/noicetom".to_string()),
        )
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .expect("PORT must be a valid integer");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;

    tracing::info!("noicetom API listening on http://{}", addr);

    axum::serve(listener, app_with_db(pool).await.into_make_service())
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
