// Event content encryption for sensitive data
// Provides AES-256-GCM encryption for sensitive event content

use std::collections::HashSet;

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use base64::{Engine as _, engine::general_purpose};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::modules::event::{EventError, Result, value_objects::RichContent};

/// Event content encryption service
#[async_trait::async_trait]
pub trait EventEncryption: Send + Sync {
    /// Encrypt sensitive content
    fn encrypt_content(&self, content: &RichContent) -> Result<EncryptedContent>;

    /// Decrypt encrypted content
    fn decrypt_content(&self, encrypted: &EncryptedContent) -> Result<RichContent>;

    /// Check if content should be encrypted based on sensitivity rules
    fn should_encrypt(&self, content: &RichContent) -> bool;
}

/// AES-256-GCM encryption implementation
pub struct AesEventEncryption {
    /// Encryption key
    key: Key<Aes256Gcm>,
    /// Sensitive keywords that trigger encryption
    sensitive_keywords: HashSet<String>,
    /// Always encrypt content above this size (in bytes)
    size_threshold: usize,
}

/// Encrypted content container
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedContent {
    /// Encrypted data (base64 encoded)
    pub data: String,
    /// Nonce used for encryption (base64 encoded)
    pub nonce: String,
    /// Encryption algorithm identifier
    pub algorithm: String,
    /// Timestamp when encrypted
    pub encrypted_at: chrono::DateTime<chrono::Utc>,
    /// Hash of original content for integrity verification
    pub content_hash: String,
}

impl AesEventEncryption {
    /// Create a new encryption service with a key
    pub fn new(key_bytes: &[u8]) -> Result<Self> {
        if key_bytes.len() != 32 {
            return Err(EventError::Configuration(
                "Encryption key must be exactly 32 bytes".to_string(),
            ));
        }

        let key = Key::<Aes256Gcm>::from_slice(key_bytes);

        // Default sensitive keywords
        let mut sensitive_keywords = HashSet::new();
        sensitive_keywords.insert("password".to_string());
        sensitive_keywords.insert("token".to_string());
        sensitive_keywords.insert("secret".to_string());
        sensitive_keywords.insert("key".to_string());
        sensitive_keywords.insert("credential".to_string());
        sensitive_keywords.insert("auth".to_string());
        sensitive_keywords.insert("session".to_string());
        sensitive_keywords.insert("cookie".to_string());
        sensitive_keywords.insert("private".to_string());
        sensitive_keywords.insert("confidential".to_string());

        Ok(Self {
            key: *key,
            sensitive_keywords,
            size_threshold: 10240, // 10KB
        })
    }

    /// Create from base64 encoded key
    pub fn from_base64_key(key_base64: &str) -> Result<Self> {
        let key_bytes = general_purpose::STANDARD
            .decode(key_base64)
            .map_err(|e| EventError::Configuration(format!("Invalid base64 key: {}", e)))?;

        Self::new(&key_bytes)
    }

    /// Generate a new random key
    pub fn generate_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);
        key
    }

    /// Add sensitive keyword
    pub fn add_sensitive_keyword(&mut self, keyword: String) {
        self.sensitive_keywords.insert(keyword.to_lowercase());
    }

    /// Remove sensitive keyword
    pub fn remove_sensitive_keyword(&mut self, keyword: &str) {
        self.sensitive_keywords.remove(&keyword.to_lowercase());
    }

    /// Set size threshold for automatic encryption
    pub fn set_size_threshold(&mut self, threshold: usize) {
        self.size_threshold = threshold;
    }

    /// Check if content contains sensitive keywords
    fn contains_sensitive_keywords(&self, content: &RichContent) -> bool {
        let title_lower = content.title().to_lowercase();

        // Check title
        for keyword in &self.sensitive_keywords {
            if title_lower.contains(keyword) {
                return true;
            }
        }

        // Check content elements
        for element in content.elements() {
            match element {
                crate::modules::event::value_objects::ContentElement::Text { content, .. } => {
                    let content_lower = content.to_lowercase();
                    for keyword in &self.sensitive_keywords {
                        if content_lower.contains(keyword) {
                            return true;
                        }
                    }
                }
                crate::modules::event::value_objects::ContentElement::Code { content, .. } => {
                    let content_lower = content.to_lowercase();
                    for keyword in &self.sensitive_keywords {
                        if content_lower.contains(keyword) {
                            return true;
                        }
                    }
                }
                _ => {} // Images, links, tables are not checked for keywords
            }
        }

        false
    }

    /// Calculate content size in bytes
    fn calculate_content_size(&self, content: &RichContent) -> usize {
        match serde_json::to_string(content) {
            Ok(serialized) => serialized.len(),
            Err(e) => {
                tracing::warn!("Failed to serialize content for size calculation: {}", e);
                0
            }
        }
    }

    /// Calculate SHA-256 hash of content
    fn calculate_content_hash(&self, content: &RichContent) -> String {
        use sha2::{Digest, Sha256};

        let serialized = match serde_json::to_string(content) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to serialize content for hashing: {}", e);
                return String::new();
            }
        };

        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        let result = hasher.finalize();

        general_purpose::STANDARD.encode(result)
    }
}

#[async_trait::async_trait]
impl EventEncryption for AesEventEncryption {
    fn encrypt_content(&self, content: &RichContent) -> Result<EncryptedContent> {
        // Serialize content to JSON
        let content_json = serde_json::to_string(content).map_err(EventError::Serialization)?;

        // Generate random nonce
        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt content
        let cipher = Aes256Gcm::new(&self.key);
        let encrypted_data = cipher
            .encrypt(nonce, content_json.as_bytes())
            .map_err(|e| EventError::Configuration(format!("Encryption failed: {}", e)))?;

        // Calculate content hash for integrity
        let content_hash = self.calculate_content_hash(content);

        Ok(EncryptedContent {
            data: general_purpose::STANDARD.encode(encrypted_data),
            nonce: general_purpose::STANDARD.encode(nonce_bytes),
            algorithm: "AES-256-GCM".to_string(),
            encrypted_at: chrono::Utc::now(),
            content_hash,
        })
    }

    fn decrypt_content(&self, encrypted: &EncryptedContent) -> Result<RichContent> {
        // Verify algorithm
        if encrypted.algorithm != "AES-256-GCM" {
            return Err(EventError::Configuration(format!(
                "Unsupported encryption algorithm: {}",
                encrypted.algorithm
            )));
        }

        // Decode base64 data
        let encrypted_data = general_purpose::STANDARD
            .decode(&encrypted.data)
            .map_err(|e| EventError::Configuration(format!("Invalid encrypted data: {}", e)))?;

        let nonce_bytes = general_purpose::STANDARD
            .decode(&encrypted.nonce)
            .map_err(|e| EventError::Configuration(format!("Invalid nonce: {}", e)))?;

        if nonce_bytes.len() != 12 {
            return Err(EventError::Configuration("Invalid nonce length".to_string()));
        }

        let nonce = Nonce::from_slice(&nonce_bytes);

        // Decrypt content
        let cipher = Aes256Gcm::new(&self.key);
        let decrypted_data = cipher
            .decrypt(nonce, encrypted_data.as_ref())
            .map_err(|e| EventError::Configuration(format!("Decryption failed: {}", e)))?;

        // Deserialize content
        let content_json = String::from_utf8(decrypted_data)
            .map_err(|e| EventError::Configuration(format!("Invalid UTF-8 data: {}", e)))?;

        let content: RichContent =
            serde_json::from_str(&content_json).map_err(EventError::Serialization)?;

        // Verify content integrity
        let calculated_hash = self.calculate_content_hash(&content);
        if calculated_hash != encrypted.content_hash {
            return Err(EventError::Configuration(
                "Content integrity verification failed".to_string(),
            ));
        }

        Ok(content)
    }

    fn should_encrypt(&self, content: &RichContent) -> bool {
        // Check for sensitive keywords
        if self.contains_sensitive_keywords(content) {
            return true;
        }

        // Check content size
        if self.calculate_content_size(content) > self.size_threshold {
            return true;
        }

        // Check metadata for sensitive markers
        if let Some(metadata) = content.metadata().get("sensitive")
            && let Some(is_sensitive) = metadata.as_bool()
        {
            return is_sensitive;
        }

        false
    }
}

/// No-op encryption implementation for testing or when encryption is disabled
pub struct NoOpEncryption;

#[async_trait::async_trait]
impl EventEncryption for NoOpEncryption {
    fn encrypt_content(&self, content: &RichContent) -> Result<EncryptedContent> {
        // Return "encrypted" content that's just base64 encoded JSON
        let content_json = serde_json::to_string(content).map_err(EventError::Serialization)?;

        Ok(EncryptedContent {
            data: general_purpose::STANDARD.encode(content_json.as_bytes()),
            nonce: general_purpose::STANDARD.encode(b"no-op-nonce"),
            algorithm: "NONE".to_string(),
            encrypted_at: chrono::Utc::now(),
            content_hash: "no-hash".to_string(),
        })
    }

    fn decrypt_content(&self, encrypted: &EncryptedContent) -> Result<RichContent> {
        if encrypted.algorithm != "NONE" {
            return Err(EventError::Configuration(
                "Cannot decrypt content encrypted with real encryption".to_string(),
            ));
        }

        let content_json = general_purpose::STANDARD
            .decode(&encrypted.data)
            .map_err(|e| EventError::Configuration(format!("Invalid data: {}", e)))?;

        let content_str = String::from_utf8(content_json)
            .map_err(|e| EventError::Configuration(format!("Invalid UTF-8: {}", e)))?;

        let content: RichContent =
            serde_json::from_str(&content_str).map_err(EventError::Serialization)?;

        Ok(content)
    }

    fn should_encrypt(&self, _content: &RichContent) -> bool {
        false // Never encrypt with no-op implementation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::event::value_objects::RichContent;

    #[test]
    fn test_key_generation() {
        let key = AesEventEncryption::generate_key();
        assert_eq!(key.len(), 32);

        // Keys should be different
        let key2 = AesEventEncryption::generate_key();
        assert_ne!(key, key2);
    }

    #[test]
    fn test_encryption_creation() {
        let key = AesEventEncryption::generate_key();
        match AesEventEncryption::new(&key) {
            Ok(_encryption) => {
                // Success
            }
            Err(e) => panic!("Failed to create encryption: {}", e),
        }

        // Test invalid key length
        let invalid_key = [0u8; 16]; // Too short
        assert!(AesEventEncryption::new(&invalid_key).is_err());
    }

    #[test]
    fn test_sensitive_keyword_detection() {
        let key = AesEventEncryption::generate_key();
        let encryption = match AesEventEncryption::new(&key) {
            Ok(enc) => enc,
            Err(e) => panic!("Failed to create encryption: {}", e),
        };

        // Test sensitive content
        let sensitive_content = RichContent::new_text(
            "User password changed".to_string(),
            "Password was updated".to_string(),
        );
        assert!(encryption.should_encrypt(&sensitive_content));

        // Test non-sensitive content
        let normal_content = RichContent::new_text(
            "Device status updated".to_string(),
            "Status changed to online".to_string(),
        );
        assert!(!encryption.should_encrypt(&normal_content));
    }
}
