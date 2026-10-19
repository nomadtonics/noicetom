use base64::{engine::general_purpose, Engine as _};
use ed25519_dalek::{PublicKey, Signature, Verifier};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use rand::RngCore;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

const CHALLENGE_TTL_SECS: i64 = 300;
const TOKEN_TTL_SECS: i64 = 86400;

type HmacSha256 = Hmac<Sha256>;

pub struct AuthManager {
    secret: String,
    challenges: Mutex<HashMap<String, (Vec<u8>, i64)>>,
}

impl AuthManager {
    pub fn new(secret: String) -> Self {
        Self { secret, challenges: Mutex::new(HashMap::new()) }
    }

    pub async fn issue_challenge(&self, username: &str) -> Vec<u8> {
        let mut rng = rand::thread_rng();
        let mut buf = vec![0u8; 32];
        rng.fill_bytes(&mut buf);
        let now = chrono::Utc::now().timestamp();
        let mut challenges = self.challenges.lock().await;
        challenges.insert(username.to_string(), (buf.clone(), now));
        buf
    }

    pub async fn verify_and_issue(&self, username: &str, signature_b64: &str, db: &tokio::sync::Mutex<crate::db::Db>) -> anyhow::Result<String> {
        let now = chrono::Utc::now().timestamp();
        let mut challenges = self.challenges.lock().await;
        let challenge_pair = challenges.get(username).ok_or_else(|| anyhow::anyhow!("no challenge"))?.clone();
        let challenge = challenge_pair.0;
        let issued_at = challenge_pair.1;
        if now.saturating_sub(issued_at) > CHALLENGE_TTL_SECS {
            challenges.remove(username);
            return Err(anyhow::anyhow!("challenge expired"));
        }
        drop(challenges);

        let db_guard = db.lock().await;
        let pk_b64 = db_guard.get_signing_pub(username)?;
        drop(db_guard);

        let pk_bytes = general_purpose::STANDARD.decode(pk_b64)?;
        let pk = PublicKey::from_bytes(&pk_bytes)?;

        let sig_bytes = general_purpose::STANDARD.decode(signature_b64)?;
        let sig = Signature::from_bytes(&sig_bytes)?;

        pk.verify(&challenge, &sig)?;

        let mut challenges = self.challenges.lock().await;
        challenges.remove(username);
        drop(challenges);

        let ts = chrono::Utc::now().timestamp();
        let mut mac = HmacSha256::new_from_slice(self.secret.as_bytes()).expect("HMAC can take key of any size");
        mac.update(username.as_bytes());
        mac.update(b":");
        mac.update(ts.to_string().as_bytes());
        let tag = mac.finalize().into_bytes();
        let mut token_bytes = Vec::new();
        token_bytes.extend_from_slice(username.as_bytes());
        token_bytes.push(b':');
        token_bytes.extend_from_slice(ts.to_string().as_bytes());
        token_bytes.push(b':');
        token_bytes.extend_from_slice(&tag);
        Ok(general_purpose::STANDARD.encode(token_bytes))
    }

    pub fn username_from_token(&self, token_b64: &str) -> anyhow::Result<String> {
        let token = general_purpose::STANDARD.decode(token_b64)?;
        let parts: Vec<&[u8]> = token.split(|b| *b == b':').collect();
        if parts.len() < 3 { return Err(anyhow::anyhow!("invalid token")); }
        let username = std::str::from_utf8(parts[0])?;
        let ts = std::str::from_utf8(parts[1])?;
        let tag = parts[2];

        let ts_num = ts.parse::<i64>()?;
        let now = chrono::Utc::now().timestamp();
        if now.saturating_sub(ts_num) > TOKEN_TTL_SECS {
            return Err(anyhow::anyhow!("token expired"));
        }

        let mut mac = HmacSha256::new_from_slice(self.secret.as_bytes()).expect("HMAC key");
        mac.update(username.as_bytes());
        mac.update(b":");
        mac.update(ts.as_bytes());
        let expected = mac.finalize().into_bytes();
        if expected.as_slice() != tag { return Err(anyhow::anyhow!("invalid token")); }

        Ok(username.to_string())
    }

    pub fn username_from_headers(&self, headers: &axum::http::HeaderMap) -> anyhow::Result<String> {
        let auth = headers.get(axum::http::header::AUTHORIZATION).ok_or_else(|| anyhow::anyhow!("missing auth"))?;
        let s = auth.to_str()?;
        if !s.starts_with("Bearer ") { return Err(anyhow::anyhow!("invalid auth header")); }
        let token = &s[7..];
        self.username_from_token(token)
    }
}
