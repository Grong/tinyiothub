// Agent Skill — DB-persisted skill wrapper.
//
// Core domain types and logic live in tinyiothub_ai::skills:
//   SkillType, SkillDefinition, parse_frontmatter, execute, glob_match

use serde::{Deserialize, Serialize};
// Re-export AI crate types for backward compatibility.
pub use tinyiothub_ai::skills::{SkillType, glob_match};

/// DB-persisted skill with workspace/agent identity.
///
/// Core logic delegates to `SkillDefinition`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSkill {
    pub id: Option<i64>,
    pub workspace_id: String,
    pub agent_id: String,
    pub skill_name: String,
    pub skill_content: String,
    pub skill_type: SkillType,
    pub paths: Option<Vec<String>>,
    pub is_hidden: bool,
}

impl AgentSkill {
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
        let paths_json: Option<Vec<String>> = paths.and_then(|p| serde_json::from_str(&p).ok());
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

    /// Delegate to AI crate SkillDefinition::parse_frontmatter.
    pub fn parse_frontmatter(content: &str) -> (Option<serde_json::Value>, &str) {
        tinyiothub_ai::skills::SkillDefinition::parse_frontmatter(content)
    }

    /// Delegate to AI crate SkillDefinition::execute.
    pub fn execute(&self, params: &serde_json::Value) -> String {
        let def = self.to_definition();
        def.execute(params)
    }

    /// Delegate to AI crate SkillDefinition::matches_path.
    pub fn matches_path(&self, file_path: &str) -> bool {
        let def = self.to_definition();
        def.matches_path(file_path)
    }

    fn to_definition(&self) -> tinyiothub_ai::skills::SkillDefinition {
        tinyiothub_ai::skills::SkillDefinition {
            skill_name: self.skill_name.clone(),
            skill_content: self.skill_content.clone(),
            skill_type: self.skill_type,
            paths: self.paths.clone(),
            is_hidden: self.is_hidden,
        }
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
}
