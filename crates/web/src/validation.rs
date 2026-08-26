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

/// 验证用户名（字母、数字、下划线，3-32个字符）
pub fn is_valid_username(username: &str) -> bool {
    let username_regex = Regex::new(r"^[a-zA-Z0-9_]{3,32}$").unwrap();
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

/// 密码策略校验的具体失败原因，便于上层映射到 i18n key
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PasswordPolicyError {
    /// 长度不足 8 位
    TooShort,
    /// 缺少字母
    NoLetter,
    /// 缺少数字
    NoDigit,
    /// 含有空白字符（空格、制表符等）
    HasWhitespace,
}

/// 结构化密码策略校验
///
/// 规则：≥ 8 位 + 至少一个字母 + 至少一个数字 + 不含空白字符。
/// 与 `is_strong_password` 共存：`is_strong_password` 仅返回布尔值，
/// 适用于不需要具体原因的快速判定；`validate_password_policy` 返回结构化原因，
/// 适用于注册/改密等需要展示精准错误提示的入口。
pub fn validate_password_policy(password: &str) -> Result<(), PasswordPolicyError> {
    if password.len() < 8 {
        return Err(PasswordPolicyError::TooShort);
    }
    if password.chars().any(|c| c.is_whitespace()) {
        return Err(PasswordPolicyError::HasWhitespace);
    }
    if !password.chars().any(|c| c.is_ascii_alphabetic()) {
        return Err(PasswordPolicyError::NoLetter);
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        return Err(PasswordPolicyError::NoDigit);
    }
    Ok(())
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
    fn test_validate_password_policy() {
        // 成功路径
        assert_eq!(validate_password_policy("password123"), Ok(()));
        assert_eq!(validate_password_policy("abc12345"), Ok(()));

        // 长度不足（< 8）
        assert_eq!(validate_password_policy("ab12"), Err(PasswordPolicyError::TooShort));
        assert_eq!(validate_password_policy("abc1234"), Err(PasswordPolicyError::TooShort));

        // 含空白字符（应在字母/数字检查前先报错）
        assert_eq!(
            validate_password_policy("pass word123"),
            Err(PasswordPolicyError::HasWhitespace)
        );
        assert_eq!(
            validate_password_policy("password\t123"),
            Err(PasswordPolicyError::HasWhitespace)
        );

        // 全数字 → 缺字母
        assert_eq!(validate_password_policy("12345678"), Err(PasswordPolicyError::NoLetter));

        // 全字母 → 缺数字
        assert_eq!(validate_password_policy("abcdefgh"), Err(PasswordPolicyError::NoDigit));
    }

    #[test]
    fn test_username_validation() {
        assert!(is_valid_username("user_123"));
        assert!(is_valid_username("abc"));
        assert!(is_valid_username("a_b_c_123_456"));
        assert!(is_valid_username("a".repeat(32).as_str())); // 上界 32
        assert!(!is_valid_username("ab"));
        assert!(!is_valid_username("user-name"));
        assert!(!is_valid_username("user name"));
        assert!(!is_valid_username("a".repeat(33).as_str())); // 越界
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
