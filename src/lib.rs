use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit as _, Nonce};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

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
    pub fn new() -> Result<Self> {
        let mut app_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        app_dir.push("noicetom");
        fs::create_dir_all(&app_dir)?;

        let path = app_dir.join("messages.json.enc");
        let key_path = app_dir.join("storage.key");

        let key = if let Ok(encoded) = std::env::var("NOICETOM_STORAGE_KEY") {
            let bytes = general_purpose::STANDARD.decode(encoded)?;
            if bytes.len() != 32 {
                return Err(anyhow!("NOICETOM_STORAGE_KEY must decode to 32 bytes"));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            arr
        } else if key_path.exists() {
            let encoded = fs::read_to_string(&key_path)?;
            let bytes = general_purpose::STANDARD.decode(encoded.trim())?;
            if bytes.len() != 32 {
                return Err(anyhow!("stored key is invalid length"));
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

    pub fn load_messages(&self) -> Result<Vec<Message>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let raw = fs::read(&self.path)?;
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        if raw.len() < 12 {
            return Err(anyhow!("encrypted payload is corrupt"));
        }

        let nonce = Nonce::from_slice(&raw[..12]);
-        let cipher = Aes256Gcm::new(Key::from_slice(&self.key));
+        let cipher = Aes256Gcm::new_from_slice(&self.key)
+            .map_err(|e| anyhow!(e.to_string()))?;
         let plaintext = cipher
             .decrypt(nonce, &raw[12..])
             .map_err(|e| anyhow!(e.to_string()))?;
         let messages: Vec<Message> = serde_json::from_slice(&plaintext)?;
         Ok(messages)
     }
 
     pub fn save_messages(&self, messages: &[Message]) -> Result<()> {
         let plaintext = serde_json::to_vec(messages)?;
-        let cipher = Aes256Gcm::new(Key::from_slice(&self.key));
+        let cipher = Aes256Gcm::new_from_slice(&self.key)
+            .map_err(|e| anyhow!(e.to_string()))?;
         let mut nonce = [0u8; 12];
         OsRng.fill_bytes(&mut nonce);
         let ciphertext = cipher
             .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
             .map_err(|e| anyhow!(e.to_string()))?;
 
         let mut out = Vec::with_capacity(12 + ciphertext.len());
         out.extend_from_slice(&nonce);
         out.extend_from_slice(&ciphertext);
         fs::write(&self.path, out)?;
         Ok(())
     }
 
     pub fn append_message(&self, message: &Message) -> Result<()> {
         let mut messages = self.load_messages()?;
         messages.push(message.clone());
         self.save_messages(&messages)?;
         Ok(())
     }
 }
