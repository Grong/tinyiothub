// Agent Skill - 技能模板模型
// 参考 Claude Code 的 SKILL.md 机制

use serde::{Deserialize, Serialize};

/// 技能类型
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[derive(Default)]
pub enum SkillType {
    #[default]
    File,    // 文件系统技能
    Bundled, // 内建打包技能
    Mcp,     // MCP Server 技能
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

/// Agent 技能
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: Option<i64>,
    pub workspace_id: String,
    pub agent_id: String,
    pub skill_name: String,
    pub skill_content: String,
    pub skill_type: SkillType,
    pub paths: Option<Vec<String>>,  // glob patterns for conditional triggers
    pub is_hidden: bool,
}

impl AgentSkill {
    /// 从数据库行创建技能
    pub fn from_row(
        id: i64,
        workspace_id: String,
        agent_id: String,
        skill_name: String,
        skill_content: String,
        skill_type: String,
        paths: Option<String>,
        is_hidden: bool,
    ) -> Self {
        let paths_json: Option<Vec<String>> = paths
            .and_then(|p| serde_json::from_str(&p).ok());

        let skill_type = match skill_type.as_str() {
            "bundled" => SkillType::Bundled,
            "mcp" => SkillType::Mcp,
            _ => SkillType::File,
        };

        Self {
            id: Some(id),
            workspace_id,
            agent_id,
            skill_name,
            skill_content,
            skill_type,
            paths: paths_json,
            is_hidden,
        }
    }

    /// 解析 Markdown 内容中的 YAML frontmatter
    /// 返回 (frontmatter, content_without_frontmatter)
    pub fn parse_frontmatter(content: &str) -> (Option<serde_json::Value>, &str) {
        if !content.starts_with("---") {
            return (None, content);
        }

        if let Some(end_pos) = content[3..].find("---") {
            let frontmatter_str = &content[3..end_pos + 3];
            let remaining = &content[end_pos + 6..];

            // 简单的 YAML frontmatter 解析
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

            return (
                Some(serde_json::Value::Object(fm)),
                remaining.trim(),
            );
        }

        (None, content)
    }

    /// 执行技能模板，替换变量
    pub fn execute(&self, params: &serde_json::Value) -> String {
        let mut content = self.skill_content.clone();

        // 替换 ${param_name} 变量
        if let Some(obj) = params.as_object() {
            for (key, value) in obj {
                let placeholder = format!("${{{}}}", key);
                let replacement = match value {
                    serde_json::Value::String(s) => {
                        // Escape ${} in values to prevent parameter injection on re-execution
                        s.replace("${", "${'${'}")
                    }
                    _ => value.to_string(),
                };
                content = content.replace(&placeholder, &replacement);
            }
        }

        content
    }

    /// 检查文件路径是否匹配技能的触发条件
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

/// 简单的 glob 匹配（仅支持 * 和 ?）
fn glob_match(pattern: &str, path: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = path.chars().collect();
    glob_match_vec(&p, &s, 0, 0)
}

fn glob_match_vec(p: &[char], s: &[char], pi: usize, si: usize) -> bool {
    // Handle ** globstar at pattern position
    if pi < p.len() && pi + 1 < p.len() && p[pi] == '*' && p[pi + 1] == '*' {
        // ** at end of pattern matches everything
        if pi + 2 >= p.len() {
            return true;
        }
        // ** followed by /
        if pi + 2 < p.len() && p[pi + 2] == '/' {
            let rest_pattern = &p[pi + 3..];
            let path_string: String = s.iter().collect();
            let path_segments: Vec<&str> = path_string.split('/').collect();
            // Try matching with 0, 1, 2, ... directories skipped
            for i in 0..=path_segments.len() {
                let rest_path: String = path_segments[i..].join("/");
                let rest_path_chars: Vec<char> = rest_path.chars().collect();
                if glob_match_vec(rest_pattern, &rest_path_chars, 0, 0) {
                    return true;
                }
            }
            return false;
        }
        // ** not followed by / - treat as two single *
        return glob_match_vec(p, s, pi + 1, si);
    }

    // Both patterns and string exhausted
    if pi >= p.len() && si >= s.len() {
        return true;
    }
    // Pattern exhausted but string not
    if pi >= p.len() {
        return false;
    }
    // String exhausted but pattern not (and not just * or ** left)
    if si >= s.len() {
        if p[pi] == '*' {
            return glob_match_vec(p, s, pi + 1, si);
        }
        return false;
    }

    match p[pi] {
        '*' => {
            // * matches zero or more chars within a segment
            if pi + 1 < p.len() && p[pi + 1] == '/' {
                // * followed by / - don't cross directory boundary
                glob_match_vec(p, s, pi + 1, si)
            } else {
                // Try matching * with zero chars
                if glob_match_vec(p, s, pi + 1, si) {
                    return true;
                }
                // Try matching * with one or more chars (but not crossing /)
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
        let skill = AgentSkill {
            id: None,
            workspace_id: "ws1".into(),
            agent_id: "default".into(),
            skill_name: "device_status".into(),
            skill_content: "设备 ${device_id} 的状态: ${status}".into(),
            skill_type: SkillType::File,
            paths: None,
            is_hidden: false,
        };

        let params = serde_json::json!({
            "device_id": "sensor_001",
            "status": "在线"
        });

        let result = skill.execute(&params);
        assert_eq!(result, "设备 sensor_001 的状态: 在线");
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "foo.rs"));
        assert!(glob_match("*.rs", "bar.rs"));
        assert!(!glob_match("*.rs", "foo.txt"));
        assert!(glob_match("device_?.rs", "device_1.rs"));
        assert!(glob_match("**/device/*.rs", "device/foo.rs"));
    }
}
