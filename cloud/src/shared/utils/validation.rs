// 输入验证模块
// 提供常用的输入验证函数

use regex::Regex;

/// 验证邮箱格式
pub fn is_valid_email(email: &str) -> bool {
    let email_regex = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
    email_regex.is_match(email)
}

/// 验证手机号（中国大陆）
pub fn is_valid_phone(phone: &str) -> bool {
    let phone_regex = Regex::new(r"^1[3-9]\d{9}$").unwrap();
    phone_regex.is_match(phone)
}

/// 验证用户名（字母、数字、下划线，3-20个字符）
pub fn is_valid_username(username: &str) -> bool {
    let username_regex = Regex::new(r"^[a-zA-Z0-9_]{3,20}$").unwrap();
    username_regex.is_match(username)
}

/// 验证密码强度（至少8位，包含数字和字母）
pub fn is_strong_password(password: &str) -> bool {
    if password.len() < 8 {
        return false;
    }
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_letter = password.chars().any(|c| c.is_ascii_alphabetic());
    has_digit && has_letter
}

/// 验证租户 slug（小写字母、数字和连字符，3-30个字符）
pub fn is_valid_slug(slug: &str) -> bool {
    let slug_regex = Regex::new(r"^[a-z0-9][a-z0-9-]{1,28}[a-z0-9]$").unwrap();
    slug_regex.is_match(slug)
}

/// 清理和验证字符串输入
pub fn sanitize_input(input: &str) -> String {
    input.trim().to_string()
}

/// 检查输入是否包含 SQL 注入风险
pub fn contains_sql_injection(input: &str) -> bool {
    let dangerous_patterns = [
        "'",
        "\"",
        ";",
        "--",
        "/*",
        "*/",
        "xp_",
        "sp_",
        "exec",
        "execute",
        "union",
        "select",
        "insert",
        "update",
        "delete",
        "drop",
        "create",
        "alter",
        "truncate",
        "script",
        "javascript",
        "<script",
    ];
    let lower = input.to_lowercase();
    dangerous_patterns.iter().any(|p| lower.contains(p))
}

/// 检查输入是否包含 XSS 风险
pub fn contains_xss(input: &str) -> bool {
    let xss_patterns = [
        "<script",
        "javascript:",
        "onerror=",
        "onclick=",
        "onload=",
        "<iframe",
        "eval(",
        "expression(",
    ];
    let lower = input.to_lowercase();
    xss_patterns.iter().any(|p| lower.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_email_validation() {
        assert!(is_valid_email("test@example.com"));
        assert!(is_valid_email("user.name+tag@example.co.uk"));
        assert!(!is_valid_email("invalid"));
        assert!(!is_valid_email("@example.com"));
    }

    #[test]
    fn test_phone_validation() {
        assert!(is_valid_phone("13812345678"));
        assert!(is_valid_phone("19987654321"));
        assert!(!is_valid_phone("12345678901"));
        assert!(!is_valid_phone("1381234567"));
    }

    #[test]
    fn test_password_strength() {
        assert!(is_strong_password("password123"));
        assert!(is_strong_password("abc12345"));
        assert!(!is_strong_password("pass"));
        assert!(!is_strong_password("12345678"));
    }

    #[test]
    fn test_username_validation() {
        assert!(is_valid_username("user_123"));
        assert!(is_valid_username("abc"));
        assert!(is_valid_username("a_b_c_123_456"));
        assert!(!is_valid_username("ab"));
        assert!(!is_valid_username("user-name"));
        assert!(!is_valid_username("user name"));
        assert!(!is_valid_username("a".repeat(21).as_str()));
    }

    #[test]
    fn test_slug_validation() {
        assert!(is_valid_slug("my-tenant"));
        assert!(is_valid_slug("abc"));
        assert!(is_valid_slug("a-b-c-123"));
        assert!(!is_valid_slug("ab"));
        assert!(!is_valid_slug("-starts-with-dash"));
        assert!(!is_valid_slug("ends-with-dash-"));
        assert!(!is_valid_slug("UPPERCASE"));
        assert!(!is_valid_slug("a".repeat(31).as_str()));
    }

    #[test]
    fn test_sanitize_input() {
        assert_eq!(sanitize_input("  hello  "), "hello");
        assert_eq!(sanitize_input("no-trim-needed"), "no-trim-needed");
        assert_eq!(sanitize_input("   "), "");
    }

    #[test]
    fn test_sql_injection_detection() {
        assert!(contains_sql_injection("'; DROP TABLE users; --"));
        assert!(contains_sql_injection(" UNION SELECT * FROM passwords"));
        assert!(contains_sql_injection("1; DELETE FROM users"));
        assert!(!contains_sql_injection("hello world"));
        assert!(!contains_sql_injection("normal_input_123"));
        assert!(!contains_sql_injection("1 OR 1=1"));
    }

    #[test]
    fn test_xss_detection() {
        assert!(contains_xss("<script>alert(1)</script>"));
        assert!(contains_xss("javascript:void(0)"));
        assert!(contains_xss("<iframe src='evil.com'>"));
        assert!(!contains_xss("normal text"));
        assert!(!contains_xss("hello <world>"));
    }
}
