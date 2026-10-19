use rusqlite::{params, Connection, OptionalExtension};
use anyhow::Result;
use std::path::Path;

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn init(&self) -> Result<()> {
        let has_users = self
            .conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type='table' AND name='users'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;

        if has_users.is_none() {
            self.conn.execute_batch(
                r#"
                CREATE TABLE users (
                    id TEXT PRIMARY KEY,
                    username TEXT UNIQUE NOT NULL,
                    signing_pub TEXT NOT NULL,
                    enc_pub TEXT NOT NULL,
                    created_at INTEGER NOT NULL
                );
                CREATE TABLE messages (
                    id TEXT PRIMARY KEY,
                    sender_username TEXT NOT NULL,
                    recipient_username TEXT NOT NULL,
                    enc_key TEXT NOT NULL,
                    ciphertext TEXT NOT NULL,
                    nonce TEXT NOT NULL,
                    alg TEXT,
                    version TEXT,
                    created_at INTEGER NOT NULL,
                    delivered INTEGER DEFAULT 0,
                    read INTEGER DEFAULT 0
                );
                "#,
            )?;
            return Ok(());
        }

        let mut stmt = self.conn.prepare("PRAGMA table_info(users)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        if !columns.iter().any(|c| c == "signing_pub") {
            self.conn.execute("ALTER TABLE users ADD COLUMN signing_pub TEXT", [])?;
        }
        if !columns.iter().any(|c| c == "enc_pub") {
            self.conn.execute("ALTER TABLE users ADD COLUMN enc_pub TEXT", [])?;
        }

        if columns.iter().any(|c| c == "public_key") {
            self.conn.execute(
                "UPDATE users SET signing_pub = public_key WHERE signing_pub IS NULL OR signing_pub = ''",
                [],
            )?;
            self.conn.execute(
                "UPDATE users SET enc_pub = public_key WHERE enc_pub IS NULL OR enc_pub = ''",
                [],
            )?;
        }

        Ok(())
    }

    pub fn insert_user(&self, username: &str, signing_pub_b64: &str, enc_pub_b64: &str) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO users (id, username, signing_pub, enc_pub, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, username, signing_pub_b64, enc_pub_b64, now],
        )?;
        Ok(())
    }

    pub fn get_public_key(&self, username: &str) -> Result<String> {
        self.get_signing_pub(username)
    }

    pub fn get_signing_pub(&self, username: &str) -> Result<String> {
        let mut stmt = self.conn.prepare("SELECT signing_pub FROM users WHERE username = ?1")?;
        let pk: Option<String> = stmt.query_row(params![username], |row| row.get(0)).optional()?;
        match pk {
            Some(pk) => Ok(pk),
            None => {
                let mut legacy = self.conn.prepare("SELECT public_key FROM users WHERE username = ?1")?;
                let legacy_pk: Option<String> = legacy.query_row(params![username], |row| row.get(0)).optional()?;
                legacy_pk.ok_or_else(|| anyhow::anyhow!("not found"))
            }
        }
    }

    pub fn get_enc_pub(&self, username: &str) -> Result<String> {
        let mut stmt = self.conn.prepare("SELECT enc_pub FROM users WHERE username = ?1")?;
        let pk: Option<String> = stmt.query_row(params![username], |row| row.get(0)).optional()?;
        match pk {
            Some(pk) => Ok(pk),
            None => {
                let mut legacy = self.conn.prepare("SELECT public_key FROM users WHERE username = ?1")?;
                let legacy_pk: Option<String> = legacy.query_row(params![username], |row| row.get(0)).optional()?;
                legacy_pk.ok_or_else(|| anyhow::anyhow!("not found"))
            }
        }
    }

    pub fn insert_message(&self, sender: &str, recipient: &str, enc_key_b64: &str, ciphertext_b64: &str, nonce_b64: &str, alg: &str, version: &str) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO messages (id, sender_username, recipient_username, enc_key, ciphertext, nonce, alg, version, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, sender, recipient, enc_key_b64, ciphertext_b64, nonce_b64, alg, version, now],
        )?;
        Ok(())
    }

    pub fn get_messages_for(&self, username: &str) -> Result<Vec<serde_json::Value>> {
        let mut stmt = self.conn.prepare("SELECT id, sender_username, enc_key, ciphertext, nonce, alg, version, created_at FROM messages WHERE recipient_username = ?1 ORDER BY created_at DESC")?;
        let rows = stmt.query_map(params![username], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "sender": row.get::<_, String>(1)?,
                "enc_key_b64": row.get::<_, String>(2)?,
                "ciphertext_b64": row.get::<_, String>(3)?,
                "nonce_b64": row.get::<_, String>(4)?,
                "alg": row.get::<_, Option<String>>(5)?,
                "version": row.get::<_, Option<String>>(6)?,
                "created_at": row.get::<_, i64>(7)?,
            }))
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}
