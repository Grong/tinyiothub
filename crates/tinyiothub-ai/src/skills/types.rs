//! Skill types — SkillType enum, SkillDefinition, and core logic.

use serde::{Deserialize, Serialize};

/// Where a skill is sourced from.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum SkillType {
    #[default]
    File,
    Bundled,
    Mcp,
}

impl std::fmt::Display for SkillType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillType::File => write!(f, "file"),
            SkillType::Bundled => write!(f, "bundled"),
            SkillType::Mcp => write!(f, "mcp"),
        }
    }
}

/// Core skill definition — storage-agnostic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub skill_name: String,
    pub skill_content: String,
    pub skill_type: SkillType,
    pub paths: Option<Vec<String>>,
    pub is_hidden: bool,
}

impl SkillDefinition {
    /// Parse YAML-like frontmatter from markdown content.
    ///
    /// Returns `(frontmatter_json, body_content)`.
    /// Frontmatter is key: value pairs between `---` delimiters.
    pub fn parse_frontmatter(content: &str) -> (Option<serde_json::Value>, &str) {
        if !content.starts_with("---") {
            return (None, content);
        }
        if let Some(end_pos) = content[3..].find("---") {
            let frontmatter_str = &content[3..end_pos + 3];
            let remaining = &content[end_pos + 6..];
            let mut fm = serde_json::Map::new();
            for line in frontmatter_str.lines() {
                if let Some(colon_pos) = line.find(':') {
                    let key = line[..colon_pos].trim();
                    let value = line[colon_pos + 1..].trim();
                    if !key.is_empty() {
                        fm.insert(key.to_string(), serde_json::json!(value));
                    }
                }
            }
            return (Some(serde_json::Value::Object(fm)), remaining.trim());
        }
        (None, content)
    }

    /// Execute template substitution: replace `${key}` with param values.
    pub fn execute(&self, params: &serde_json::Value) -> String {
        let mut content = self.skill_content.clone();
        if let Some(obj) = params.as_object() {
            for (key, value) in obj {
                let placeholder = format!("${{{}}}", key);
                let replacement = match value {
                    serde_json::Value::String(s) => s.replace("${", "${'${'}"),
                    _ => value.to_string(),
                };
                content = content.replace(&placeholder, &replacement);
            }
        }
        content
    }

    /// Check whether this skill applies to a given file path (glob matching).
    pub fn matches_path(&self, file_path: &str) -> bool {
        let paths = match &self.paths {
            Some(p) => p,
            None => return false,
        };
        for pattern in paths {
            if glob_match(pattern, file_path) {
                return true;
            }
        }
        false
    }
}

// ── Glob matching ──

pub fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    glob_match_vec(&p, &s, 0, 0)
}

fn glob_match_vec(p: &[char], s: &[char], pi: usize, si: usize) -> bool {
    if pi < p.len() && pi + 1 < p.len() && p[pi] == '*' && p[pi + 1] == '*' {
        if pi + 2 >= p.len() {
            return true;
        }
        if pi + 2 < p.len() && p[pi + 2] == '/' {
            let rest_pattern = &p[pi + 3..];
            let path_string: String = s.iter().collect();
            let path_segments: Vec<&str> = path_string.split('/').collect();
            for i in 0..=path_segments.len() {
                let rest_path: String = path_segments[i..].join("/");
                let rest_path_chars: Vec<char> = rest_path.chars().collect();
                if glob_match_vec(rest_pattern, &rest_path_chars, 0, 0) {
                    return true;
                }
            }
            return false;
        }
        return glob_match_vec(p, s, pi + 1, si);
    }
    if pi >= p.len() && si >= s.len() {
        return true;
    }
    if pi >= p.len() {
        return false;
    }
    if si >= s.len() {
        return if p[pi] == '*' {
            glob_match_vec(p, s, pi + 1, si)
        } else {
            false
        };
    }
    match p[pi] {
        '*' => {
            if pi + 1 < p.len() && p[pi + 1] == '/' {
                glob_match_vec(p, s, pi + 1, si)
            } else {
                if glob_match_vec(p, s, pi + 1, si) {
                    return true;
                }
                if s[si] != '/' && glob_match_vec(p, s, pi, si + 1) {
                    return true;
                }
                false
            }
        }
        '?' => glob_match_vec(p, s, pi + 1, si + 1),
        c if c == s[si] => glob_match_vec(p, s, pi + 1, si + 1),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute() {
        let skill = SkillDefinition {
            skill_name: "device_status".into(),
            skill_content: "设备 ${device_id} 的状态: ${status}".into(),
            skill_type: SkillType::File,
            paths: None,
            is_hidden: false,
        };
        let params = serde_json::json!({ "device_id": "sensor_001", "status": "在线" });
        assert_eq!(skill.execute(&params), "设备 sensor_001 的状态: 在线");
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "foo.rs"));
        assert!(glob_match("*.rs", "bar.rs"));
        assert!(!glob_match("*.rs", "foo.txt"));
        assert!(glob_match("device_?.rs", "device_1.rs"));
        assert!(glob_match("**/device/*.rs", "device/foo.rs"));
    }

    #[test]
    fn test_parse_frontmatter() {
        let content = r#"---
name: alarm-management
description: Manage alarms
version: 1.0.0
---

# Alarm Management"#;

        let (fm, body) = SkillDefinition::parse_frontmatter(content);
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert_eq!(fm.get("name").unwrap().as_str().unwrap(), "alarm-management");
        assert_eq!(fm.get("description").unwrap().as_str().unwrap(), "Manage alarms");
        assert!(body.contains("Alarm Management"));
    }

    #[test]
    fn test_parse_frontmatter_none() {
        let content = "# Plain skill without frontmatter";
        let (fm, body) = SkillDefinition::parse_frontmatter(content);
        assert!(fm.is_none());
        assert_eq!(body.trim(), "# Plain skill without frontmatter");
    }
}
