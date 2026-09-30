//! 工作区原则加载——自治大脑的用户文件层（与 chat 同一三层回退）。
//!
//! SOUL.md（怎么做人）+ AGENTS.md（交互规则），按 workspace 覆盖 →
//! _default 共享 → 内嵌默认的回退加载。代码锁定的宪法段不受影响——
//! 用户文件只能塑造行为风格，不能放松安全纪律。

use std::path::PathBuf;

use crate::prompt::paths;

/// 大脑工作区文件（两个）：SOUL.md = 行为原则，AGENTS.md = 交互规则。
const PRINCIPLE_FILES: [(&str, &str); 2] = [("SOUL.md", "行为原则"), ("AGENTS.md", "交互规则")];

/// 加载某 workspace 的原则段文本；无文件时返回空串（段落整体省略）。
/// 同步读（文件极小；build_prompt 是同步上下文）。
pub fn load_principles(workspace_id: &str) -> String {
    let ws_dir = paths::workspace_dir(workspace_id);
    let shared = paths::shared_agent_base_dir();
    let mut out = String::new();
    for (filename, label) in PRINCIPLE_FILES {
        if let Some(content) = read_first_existing(&[ws_dir.join(filename), shared.join(filename)]) {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&format!("## {label}\n{}", content.trim_end()));
        }
    }
    out
}

fn read_first_existing(candidates: &[PathBuf]) -> Option<String> {
    candidates
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principles_fall_back_to_shared_default() {
        // _default/SOUL.md 在仓库 data/agents/_default 真实存在（部署态）。
        // workspace 目录不存在时也必须命中共享层，不得返回空。
        let text = load_principles(paths::DEFAULT_WORKSPACE_ID);
        if paths::shared_agent_base_dir().join("SOUL.md").exists() {
            assert!(text.contains("行为原则"), "shared SOUL.md must load: {text:?}");
        }
    }
}
