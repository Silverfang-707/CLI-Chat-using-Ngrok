use serde::{Deserialize, Serialize};
use aes_gcm::{aead::{Aead, KeyInit, OsRng}, Aes256Gcm, Key, Nonce};
use aes_gcm::aead::AeadCore;
use sha2::{Sha256, Digest};
use pbkdf2::pbkdf2_hmac;
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub use aes_gcm::Key as AesKey;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub action: String,
    pub room: String,
    pub name: String,
    pub target: String, // RESTORED: Vital for true E2EE whispers
    pub content: String,
    pub auth: String,
}

pub fn derive_key(password: &str, room: &str) -> Key<Aes256Gcm> {
    let salt = format!("airaa_room_key_{}", room.to_lowercase());
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 100_000, &mut key_bytes);
    *Key::<Aes256Gcm>::from_slice(&key_bytes)
}

// RESTORED: Target-salted key derivation for true private routing
pub fn derive_whisper_key(password: &str, room: &str, target: &str) -> Key<Aes256Gcm> {
    let salt = format!("airaa_whisper_{}_{}", room.to_lowercase(), target.to_lowercase());
    let mut key_bytes = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt.as_bytes(), 100_000, &mut key_bytes);
    *Key::<Aes256Gcm>::from_slice(&key_bytes)
}

pub fn derive_auth_hash(password: &str, room: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(b"_airaa_server_bouncer_salt_");
    hasher.update(room.to_lowercase().as_bytes());
    let result = hasher.finalize();
    STANDARD.encode(result)
}

pub fn encrypt_with_key(key: &Key<Aes256Gcm>, plaintext: &str) -> Result<String, String> {
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher.encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {:?}", e))?;

    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(STANDARD.encode(combined))
}

pub fn decrypt_with_key(key: &Key<Aes256Gcm>, encoded_ciphertext: &str) -> Result<String, String> {
    if encoded_ciphertext.is_empty() { return Ok(String::new()); }
    let combined = STANDARD.decode(encoded_ciphertext).map_err(|_| "Base64 decode failed".to_string())?;
    if combined.len() < 12 { return Err("Payload too short".to_string()); }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(key);

    let plaintext_bytes = cipher.decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed (wrong password/target)".to_string())?;

    String::from_utf8(plaintext_bytes).map_err(|_| "Invalid UTF-8 in decrypted payload".to_string())
}