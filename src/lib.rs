use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, KeyInit as _, Nonce,
};
use base64::{engine::general_purpose, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs,
    path::PathBuf,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub content: String,
    pub created_at: String,
}

pub struct Storage {
    path: PathBuf,
    key: [u8; 32],
}

impl Storage {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let mut app_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        app_dir.push("noicetom");
        fs::create_dir_all(&app_dir)?;

        let path = app_dir.join("messages.json.enc");
        let key_path = app_dir.join("storage.key");

        let key = if let Ok(encoded) = std::env::var("NOICETOM_STORAGE_KEY") {
            let bytes = general_purpose::STANDARD.decode(encoded)?;
            if bytes.len() != 32 {
                return Err("NOICETOM_STORAGE_KEY must decode to 32 bytes".into());
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        } else if key_path.exists() {
            let encoded = fs::read_to_string(&key_path)?;
            let bytes = general_purpose::STANDARD.decode(encoded.trim())?;
            if bytes.len() != 32 {
                return Err("stored key is invalid length".into());
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        } else {
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            fs::write(&key_path, general_purpose::STANDARD.encode(key) + "\n")?;
            key
        };

        Ok(Self { path, key })
    }

    pub fn load_messages(&self) -> Result<Vec<Message>, Box<dyn Error>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let raw = fs::read(&self.path)?;
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        if raw.len() < 12 {
            return Err("encrypted payload is corrupt".into());
        }

        let nonce = Nonce::from_slice(&raw[..12]);
        let cipher = Aes256Gcm::new(Key::from_slice(&self.key));
        let plaintext = cipher.decrypt(nonce, &raw[12..])?;
        let messages: Vec<Message> = serde_json::from_slice(&plaintext)?;
        Ok(messages)
    }

    pub fn save_messages(&self, messages: &[Message]) -> Result<(), Box<dyn Error>> {
        let plaintext = serde_json::to_vec(messages)?;
        let cipher = Aes256Gcm::new(Key::from_slice(&self.key));
        let mut nonce = [0u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher.encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())?;

        let mut out = Vec::with_capacity(12 + ciphertext.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ciphertext);
        fs::write(&self.path, out)?;
        Ok(())
    }

    pub fn append_message(&self, message: &Message) -> Result<(), Box<dyn Error>> {
        let mut messages = self.load_messages()?;
        messages.push(message.clone());
        self.save_messages(&messages)?;
        Ok(())
    }
}
