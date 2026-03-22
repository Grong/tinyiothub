// 密码哈希模块
// 使用 bcrypt 进行安全密码哈希

use bcrypt::{hash, verify, DEFAULT_COST};

/// 哈希密码
pub fn hash_password(password: &str) -> Result<String, String> {
    hash(password, DEFAULT_COST)
        .map_err(|e| format!("Failed to hash password: {}", e))
}

/// 验证密码
pub fn verify_password(password: &str, hash: &str) -> Result<bool, String> {
    verify(password, hash)
        .map_err(|e| format!("Failed to verify password: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let password = "test_password_123";
        let hashed = hash_password(password).unwrap();
        
        assert!(verify_password(password, &hashed).is_ok());
        assert!(!verify_password("wrong_password", &hashed).is_ok());
    }
}
