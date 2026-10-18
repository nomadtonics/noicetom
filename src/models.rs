use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MessageRecord {
    pub id: String,
    pub sender: String,
    pub enc_key_b64: String,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub alg: Option<String>,
    pub version: Option<String>,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    pub public_key_b64: String,
}
