use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::Mutex;
use tracing_subscriber;

mod db;
mod models;
mod auth;

use db::Db;
use models::*;
use auth::AuthManager;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Db>>,
    auth: Arc<AuthManager>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let db_path = std::env::var("NOICETOM_STORAGE_PATH").unwrap_or_else(|_| "data/noicetom.sqlite".into());
    let server_secret = std::env::var("SERVER_SECRET").unwrap_or_else(|_| "dev-secret".into());

    let db = Db::open(&db_path)?;
    db.init()?;

    let state = AppState {
        db: Arc::new(Mutex::new(db)),
        auth: Arc::new(AuthManager::new(server_secret)),
    };

    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/register", post(register))
        .route("/api/auth/challenge", post(challenge))
        .route("/api/auth/verify", post(verify))
        .route("/api/keys/:username", get(get_key))
        .route("/api/messages/send", post(send_message))
        .route("/api/messages/inbox", get(inbox))
        .with_state(state.clone());

    let port: u16 = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string()).parse().unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr).serve(app.into_make_service()).await?;

    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    signing_pub_b64: String,
    enc_pub_b64: String,
}

async fn register(State(state): State<AppState>, Json(req): Json<RegisterRequest>) -> impl IntoResponse {
    // validate base64 and key lengths (Ed25519 & X25519 are 32 bytes)
    let signing_bytes = match general_purpose::STANDARD.decode(&req.signing_pub_b64) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": "invalid signing_pub_b64" }))),
    };
    let enc_bytes = match general_purpose::STANDARD.decode(&req.enc_pub_b64) {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": "invalid enc_pub_b64" }))),
    };
    if signing_bytes.len() != 32 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": "signing public key must be 32 bytes" })));
    }
    if enc_bytes.len() != 32 {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "ok": false, "error": "encryption public key must be 32 bytes" })));
    }

    let db_guard = state.db.clone();
    let db = db_guard.lock().await;
    match db.insert_user(&req.username, &req.signing_pub_b64, &req.enc_pub_b64) {
        Ok(_) => (StatusCode::CREATED, Json(serde_json::json!({ "ok": true }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "ok": false, "error": e.to_string() }))),
    }
}

#[derive(Deserialize)]
struct ChallengeRequest {
    username: String,
}

async fn challenge(State(state): State<AppState>, Json(req): Json<ChallengeRequest>) -> impl IntoResponse {
    let ch = state.auth.issue_challenge(&req.username).await;
    (StatusCode::OK, Json(serde_json::json!({ "challenge": general_purpose::STANDARD.encode(ch) })))
}

#[derive(Deserialize)]
struct VerifyRequest {
    username: String,
    signature_b64: String,
}

async fn verify(State(state): State<AppState>, Json(req): Json<VerifyRequest>) -> impl IntoResponse {
    match state.auth.verify_and_issue(&req.username, &req.signature_b64, &state.db).await {
        Ok(token) => (StatusCode::OK, Json(serde_json::json!({ "token": token }))),
        Err(e) => (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn get_key(State(state): State<AppState>, Path(username): Path<String>) -> impl IntoResponse {
    let db_guard = state.db.clone();
    let db = db_guard.lock().await;
    match db.get_enc_pub(&username) {
        Ok(pk) => (StatusCode::OK, Json(serde_json::json!({ "enc_pub_b64": pk }))),
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))),
    }
}

#[derive(Deserialize)]
struct SendMessageRequest {
    to_username: String,
    enc_key_b64: String,
    ciphertext_b64: String,
    nonce_b64: String,
    alg: Option<String>,
    version: Option<String>,
}

async fn send_message(State(state): State<AppState>, Json(req): Json<SendMessageRequest>, headers: HeaderMap) -> impl IntoResponse {
    // auth
    let auth = &state.auth;
    let user = match auth.username_from_headers(&headers) {
        Ok(u) => u,
        Err(_) => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))),
    };

    let db_guard = state.db.clone();
    let db = db_guard.lock().await;
    match db.insert_message(&user, &req.to_username, &req.enc_key_b64, &req.ciphertext_b64, &req.nonce_b64, req.alg.as_deref().unwrap_or(""), req.version.as_deref().unwrap_or("")) {
        Ok(_) => (StatusCode::CREATED, Json(serde_json::json!({ "ok": true }))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}

async fn inbox(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let auth = &state.auth;
    let username = match auth.username_from_headers(&headers) {
        Ok(u) => u,
        Err(_) => return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "unauthorized" }))),
    };

    let db_guard = state.db.clone();
    let db = db_guard.lock().await;
    match db.get_messages_for(&username) {
        Ok(list) => (StatusCode::OK, Json(serde_json::Value::Array(list))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))),
    }
}
