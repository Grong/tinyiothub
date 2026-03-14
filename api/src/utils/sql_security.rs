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
    }

    #[test]
    fn test_is_safe_identifier() {
        assert!(is_safe_identifier("users"));
        assert!(is_safe_identifier("user_name"));
        assert!(is_safe_identifier("user123"));
        assert!(!is_safe_identifier(""));
        assert!(!is_safe_identifier("user;drop table"));
        assert!(!is_safe_identifier("user name"));
    }
}
