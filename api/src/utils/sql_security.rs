// SQL 注入防护模块
// 提供安全的 SQL 查询辅助函数

use sqlx::{Database, Pool, Row};

/// 安全地将字符串插入 SQL 查询（转义单引号）
pub fn escape_sql_string(input: &str) -> String {
    input.replace('\'', "''")
}

/// 构建安全的 LIKE 查询模式（转义特殊字符）
pub fn escape_like_pattern(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// 验证字符串是否可以安全地用于标识符（表名、列名等）
pub fn is_safe_identifier(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// SQL 查询构建辅助 - 用于构建参数化查询
/// 
/// # 示例
/// ```
/// let query = build_where_clause(vec![
///     ("username", Some("john")),
///     ("email", Some("john@example.com")),
/// ]);
/// // 结果: "WHERE username = ? AND email = ?"
/// ```
pub fn build_where_clause(filters: Vec<(&str, Option<String>)>) -> (String, Vec<String>) {
    let mut conditions = Vec::new();
    let mut values = Vec::new();

    for (field, value) in filters {
        if let Some(v) = value {
            if !v.is_empty() {
                conditions.push(format!("{} = ?", field));
                values.push(v);
            }
        }
    }

    if conditions.is_empty() {
        return (String::new(), values);
    }

    let where_clause = format!("WHERE {}", conditions.join(" AND "));
    (where_clause, values)
}

/// 分页查询辅助
pub fn build_pagination(page: Option<u32>, page_size: Option<u32>) -> (String, u32, u32) {
    let page = page.unwrap_or(1).max(1);
    let page_size = page_size.unwrap_or(20).min(100);
    let offset = (page - 1) * page_size;

    (
        format!("LIMIT {} OFFSET {}", page_size, offset),
        page,
        page_size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_sql_string() {
        assert_eq!(escape_sql_string("test"), "test");
        assert_eq!(escape_sql_string("it's"), "it''s");
        assert_eq!(escape_sql_string("test'value"), "test''value");
        assert_eq!(escape_sql_string("O'Brien"), "O''Brien");
        assert_eq!(escape_sql_string(""), "");
    }

    #[test]
    fn test_escape_like_pattern() {
        assert_eq!(escape_like_pattern("test"), "test");
        assert_eq!(escape_like_pattern("test%"), "test\\%");
        assert_eq!(escape_like_pattern("test_"), "test\\_");
        assert_eq!(escape_like_pattern("100%"), "100\\%");
        assert_eq!(escape_like_pattern("a_b"), "a\\_b");
    }

    #[test]
    fn test_is_safe_identifier() {
        assert!(is_safe_identifier("users"));
        assert!(is_safe_identifier("user_name"));
        assert!(is_safe_identifier("user123"));
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("user;drop table"));
        assert!(!is_safe_identifier("user name"));
        assert!(!is_safe_identifier("user-name"));
        let long_name = "a".repeat(65);
        assert!(!is_safe_identifier(&long_name)); // too long
    }

    #[test]
    fn test_build_where_clause() {
        let (clause, values) = build_where_clause(vec![
            ("name", Some("test".to_string())),
            ("status", Some("active".to_string())),
        ]);
        assert_eq!(clause, "WHERE name = ? AND status = ?");
        assert_eq!(values, vec!["test", "active"]);
    }

    #[test]
    fn test_build_where_clause_empty() {
        let (clause, values) = build_where_clause(vec![]);
        assert_eq!(clause, "");
        assert!(values.is_empty());
    }

    #[test]
    fn test_build_where_clause_skips_empty() {
        let (clause, values) = build_where_clause(vec![
            ("name", Some("test".to_string())),
            ("status", None),
            ("type", Some("".to_string())),
        ]);
        assert_eq!(clause, "WHERE name = ?");
        assert_eq!(values, vec!["test"]);
    }

    #[test]
    fn test_build_pagination_default() {
        let (limit, page, size) = build_pagination(None, None);
        assert_eq!(limit, "LIMIT 20 OFFSET 0");
        assert_eq!(page, 1);
        assert_eq!(size, 20);
    }

    #[test]
    fn test_build_pagination_custom() {
        let (limit, page, size) = build_pagination(Some(3), Some(50));
        assert_eq!(limit, "LIMIT 50 OFFSET 100");
        assert_eq!(page, 3);
        assert_eq!(size, 50);
    }

    #[test]
    fn test_build_pagination_min_max() {
        // page 0 becomes 1
        let (limit, page, size) = build_pagination(Some(0), Some(10));
        assert_eq!(limit, "LIMIT 10 OFFSET 0");
        assert_eq!(page, 1);
        assert_eq!(size, 10);

        // page_size > 100 capped to 100
        let (limit, _, _) = build_pagination(Some(1), Some(200));
        assert_eq!(limit, "LIMIT 100 OFFSET 0");
    }
}
