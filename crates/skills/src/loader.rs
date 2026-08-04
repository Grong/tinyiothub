//! Unified skill loading, index rendering, and per-turn trigger resolution.
//!
//! One source of truth for how a `*.md` skill file is discovered, how it is
//! injected into the system prompt (index layer), and how a `/trigger` in an
//! incoming message pulls the full skill body into the current turn.
//!
//! Injection is driven by the skill's frontmatter `inject` field:
//! - `always`  — full body always rendered into the system-prompt skill section
//! - `index`   — one-line index entry only; the LLM loads full body via `get_skill`
//! - `trigger` — not in the base prompt; full body prepended to the turn when the message starts
//!   with the skill's `trigger` token

use std::collections::BTreeMap;
use std::path::Path;

use crate::types::SkillDefinition;

/// How a skill's content enters the model's context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InjectMode {
    /// Full body always present in the system-prompt skill section.
    Always,
    /// Only an index entry in the system prompt; full body via `get_skill`.
    #[default]
    Index,
    /// Full body prepended to the turn when the trigger token is present.
    Trigger,
}

impl InjectMode {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "always" => InjectMode::Always,
            "trigger" => InjectMode::Trigger,
            _ => InjectMode::Index,
        }
    }
}

/// A skill parsed from a `*.md` file, with injection metadata resolved.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    /// File stem / `name:` frontmatter — the id used by `get_skill`.
    pub name: String,
    /// Display title (first `# ` heading, falling back to `name`).
    pub title: String,
    /// One-line description for the index.
    pub description: String,
    pub version: String,
    pub inject: InjectMode,
    /// Trigger token including the leading slash, e.g. `/workspace`.
    pub trigger: Option<String>,
    /// Full markdown body (frontmatter stripped).
    pub body: String,
}

/// Result of matching a `/trigger` against the loaded skills.
#[derive(Debug, Clone)]
pub struct TriggerHit {
    pub name: String,
    /// The full skill body to inject.
    pub body: String,
    /// The user's message with the trigger token stripped.
    pub cleaned_message: String,
}

/// Load skills from a priority-ordered list of directories, merged by name.
///
/// Earlier directories win: a `workspace/agent` skill shadows a global skill of
/// the same name, but skills unique to the global dir are still included. This
/// removes the "first non-empty dir wins, rest ignored" trap of the old loaders.
pub fn load_skills_from_dirs(dirs: &[std::path::PathBuf]) -> Vec<LoadedSkill> {
    let mut by_name: BTreeMap<String, LoadedSkill> = BTreeMap::new();
    for dir in dirs {
        for skill in read_skill_dir(dir) {
            by_name.entry(skill.name.clone()).or_insert(skill);
        }
    }
    by_name.into_values().collect()
}

/// Skill files are prompt text; anything larger blows the prompt budget.
const MAX_SKILL_FILE_BYTES: u64 = 256 * 1024;

fn read_skill_dir(dir: &Path) -> Vec<LoadedSkill> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        // Symlinks can point outside the skill dir and leak host files into
        // the prompt — refuse them outright.
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) {
            continue;
        }
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        match std::fs::metadata(&path) {
            Ok(m) if m.len() <= MAX_SKILL_FILE_BYTES => {}
            _ => continue,
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if let Some(skill) = parse_skill(&stem, &content) {
            out.push(skill);
        }
    }
    out
}

/// Parse a single skill file into a `LoadedSkill`. Returns `None` for empty bodies.
pub fn parse_skill(file_stem: &str, content: &str) -> Option<LoadedSkill> {
    let (fm, body) = SkillDefinition::parse_frontmatter(content);
    let body = body.trim();
    if body.is_empty() {
        return None;
    }

    let fm_str = |key: &str| -> Option<String> {
        fm.as_ref()
            .and_then(|f| f.get(key))
            .and_then(|v| v.as_str())
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
    };

    let name = fm_str("name").unwrap_or_else(|| file_stem.to_string());
    let version = fm_str("version").unwrap_or_default();
    let inject = fm_str("inject").map(|s| InjectMode::parse(&s)).unwrap_or_default();

    let trigger = fm_str("trigger").map(|t| if t.starts_with('/') { t } else { format!("/{}", t) });

    let title = body
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").trim().to_string())
        .unwrap_or_else(|| name.clone());

    let description = fm_str("description").unwrap_or_else(|| {
        body.lines()
            .skip_while(|l| l.starts_with('#') || l.trim().is_empty())
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().to_string())
            .unwrap_or_default()
    });

    Some(LoadedSkill {
        name,
        title,
        description,
        version,
        inject,
        trigger,
        body: body.to_string(),
    })
}

/// Render the system-prompt skill section: `always` skills in full, plus a
/// compact index of the rest. Returns an empty string when there are no skills.
pub fn build_skill_index_prompt(skills: &[LoadedSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut full = String::new();
    let mut index: Vec<String> = Vec::new();

    for s in skills {
        match s.inject {
            InjectMode::Always => {
                full.push_str(&s.body);
                full.push_str("\n\n");
            }
            InjectMode::Index | InjectMode::Trigger => {
                let hint = match (&s.trigger, s.inject) {
                    (Some(t), InjectMode::Trigger) => format!(" — 发送 `{}` 触发", t),
                    _ => String::new(),
                };
                index.push(format!("- **{}** (`{}`) — {}{}", s.title, s.name, s.description, hint));
            }
        }
    }

    let mut out = String::from("## 技能（Skills）\n");
    if !full.is_empty() {
        out.push_str(full.trim_end());
        out.push_str("\n\n");
    }
    if !index.is_empty() {
        out.push_str("以下技能可用,使用 `get_skill` 工具按名称加载完整内容:\n\n");
        for entry in &index {
            out.push_str(entry);
            out.push('\n');
        }
    }
    out.trim_end().to_string()
}

/// If `message` starts with a `/trigger` token matching a `Trigger`-mode skill,
/// return the skill body and the message with the token stripped.
///
/// The trigger set is a whitelist of loaded skills' `trigger` fields, so this is
/// inherently safe against path traversal / arbitrary file access.
pub fn resolve_trigger(message: &str, skills: &[LoadedSkill]) -> Option<TriggerHit> {
    let trimmed = message.trim_start();
    let first_token: &str = trimmed.split(|c: char| c.is_whitespace()).next().unwrap_or("");
    if !first_token.starts_with('/') {
        return None;
    }

    let skill = skills
        .iter()
        .find(|s| s.inject == InjectMode::Trigger && s.trigger.as_deref() == Some(first_token))?;

    let cleaned = trimmed[first_token.len()..].trim().to_string();
    Some(TriggerHit {
        name: skill.name.clone(),
        body: skill.body.clone(),
        cleaned_message: cleaned,
    })
}

/// Wrap an injected skill body + user message for a single turn.
pub fn wrap_injected_skill(name: &str, body: &str, user_message: &str) -> String {
    if user_message.is_empty() {
        format!("<skill name=\"{}\">\n{}\n</skill>", name, body)
    } else {
        format!("<skill name=\"{}\">\n{}\n</skill>\n\n{}", name, body, user_message)
    }
}

/// Strip a leading `<skill …>…</skill>` block (as produced by
/// [`wrap_injected_skill`]) so chat history shows only the user's text.
pub fn strip_injected_skill(content: &str) -> String {
    let trimmed = content.trim_start();
    // Verify it looks like our injected block: starts with `<skill name="...">`.
    if !trimmed.starts_with("<skill name=\"") {
        return content.to_string();
    }
    // Find the matching close tag. Prefer the "</skill>\n\n" delimiter used
    // when a user message follows; fall back to a trailing `</skill>` so a
    // bare trigger still strips cleanly. This avoids mis-splitting if the skill
    // body happens to contain the literal string "</skill>".
    for marker in ["</skill>\n\n", "\n</skill>"] {
        if let Some(idx) = trimmed.find(marker) {
            return trimmed[idx + marker.len()..].trim_start().to_string();
        }
    }
    content.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(name: &str, inject: &str, trigger: Option<&str>) -> LoadedSkill {
        let fm_trigger = trigger.map(|t| format!("trigger: {}\n", t)).unwrap_or_default();
        let content = format!(
            "---\nname: {name}\ndescription: desc for {name}\ninject: {inject}\n{fm_trigger}version: \"1.0\"\n---\n\n# {name} Title\n\nBody of {name}."
        );
        parse_skill(name, &content).unwrap()
    }

    #[test]
    fn parse_reads_inject_and_trigger() {
        let s = skill("workspace", "trigger", Some("/workspace"));
        assert_eq!(s.name, "workspace");
        assert_eq!(s.inject, InjectMode::Trigger);
        assert_eq!(s.trigger.as_deref(), Some("/workspace"));
        assert_eq!(s.title, "workspace Title");
        assert_eq!(s.description, "desc for workspace");
    }

    #[test]
    fn trigger_normalizes_missing_slash() {
        let s = skill("ws", "trigger", Some("ws"));
        assert_eq!(s.trigger.as_deref(), Some("/ws"));
    }

    #[test]
    fn inject_defaults_to_index() {
        let content = "---\nname: x\ndescription: d\n---\n\n# X\n\nbody";
        let s = parse_skill("x", content).unwrap();
        assert_eq!(s.inject, InjectMode::Index);
    }

    #[test]
    fn empty_body_is_skipped() {
        let content = "---\nname: x\n---\n";
        assert!(parse_skill("x", content).is_none());
    }

    #[test]
    fn index_prompt_renders_always_full_and_others_as_index() {
        let skills = vec![
            skill("overview", "always", None),
            skill("workspace", "trigger", Some("/workspace")),
            skill("troubleshoot", "index", None),
        ];
        let out = build_skill_index_prompt(&skills);
        assert!(out.contains("Body of overview.")); // always → full
        assert!(!out.contains("Body of workspace.")); // trigger → index only
        assert!(out.contains("`workspace`"));
        assert!(out.contains("发送 `/workspace` 触发"));
        assert!(out.contains("`troubleshoot`"));
    }

    #[test]
    fn index_prompt_empty_when_no_skills() {
        assert_eq!(build_skill_index_prompt(&[]), "");
    }

    #[test]
    fn resolve_trigger_hits_with_body() {
        let skills = vec![skill("workspace", "trigger", Some("/workspace"))];
        let hit = resolve_trigger("/workspace\n查看综合情况", &skills).unwrap();
        assert_eq!(hit.name, "workspace");
        assert_eq!(hit.cleaned_message, "查看综合情况");
        assert!(hit.body.contains("Body of workspace."));
    }

    #[test]
    fn resolve_trigger_bare_token_yields_empty_message() {
        let skills = vec![skill("workspace", "trigger", Some("/workspace"))];
        let hit = resolve_trigger("/workspace", &skills).unwrap();
        assert_eq!(hit.cleaned_message, "");
    }

    #[test]
    fn resolve_trigger_no_match_returns_none() {
        let skills = vec![skill("workspace", "trigger", Some("/workspace"))];
        assert!(resolve_trigger("hello world", &skills).is_none());
        assert!(resolve_trigger("/unknown do stuff", &skills).is_none());
    }

    #[test]
    fn resolve_trigger_ignores_non_trigger_mode() {
        let skills = vec![skill("workspace", "index", Some("/workspace"))];
        assert!(resolve_trigger("/workspace hi", &skills).is_none());
    }

    #[test]
    fn wrap_and_strip_roundtrip() {
        let wrapped = wrap_injected_skill("workspace", "SKILL BODY", "user text");
        assert!(wrapped.contains("SKILL BODY"));
        assert_eq!(strip_injected_skill(&wrapped), "user text");
    }

    #[test]
    fn wrap_bare_and_strip() {
        let wrapped = wrap_injected_skill("workspace", "SKILL BODY", "");
        assert_eq!(strip_injected_skill(&wrapped), "");
    }

    #[test]
    fn strip_passthrough_when_no_block() {
        assert_eq!(strip_injected_skill("just a message"), "just a message");
    }

    #[test]
    fn load_merges_by_name_priority() {
        let dir_a = std::env::temp_dir().join(format!("skills_a_{}", uuid::Uuid::new_v4()));
        let dir_b = std::env::temp_dir().join(format!("skills_b_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        // dir_a (priority) overrides "shared"; dir_b adds "global"
        std::fs::write(dir_a.join("shared.md"), "---\nname: shared\n---\n# S\n\nFROM_A").unwrap();
        std::fs::write(dir_b.join("shared.md"), "---\nname: shared\n---\n# S\n\nFROM_B").unwrap();
        std::fs::write(dir_b.join("global.md"), "---\nname: global\n---\n# G\n\nGLOBAL").unwrap();

        let skills = load_skills_from_dirs(&[dir_a.clone(), dir_b.clone()]);
        let shared = skills.iter().find(|s| s.name == "shared").unwrap();
        assert!(shared.body.contains("FROM_A"));
        assert!(skills.iter().any(|s| s.name == "global"));

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    #[test]
    fn load_skips_files_over_size_limit() {
        // A multi-MB "skill" would blow the prompt budget; refuse to load it.
        let dir = std::env::temp_dir().join(format!("skills_big_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let huge = format!("---\nname: big\n---\n# B\n\n{}", "x".repeat(512 * 1024));
        std::fs::write(dir.join("big.md"), huge).unwrap();
        std::fs::write(dir.join("ok.md"), "---\nname: ok\n---\n# O\n\nfine").unwrap();

        let skills = load_skills_from_dirs(std::slice::from_ref(&dir));
        assert!(
            skills.iter().all(|s| s.name != "big"),
            "oversized skill file must be skipped"
        );
        assert!(skills.iter().any(|s| s.name == "ok"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn load_skips_symlinked_files() {
        // A symlinked skill can leak arbitrary host files into the prompt.
        let dir = std::env::temp_dir().join(format!("skills_link_{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("skills_outside_{}.md", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&outside, "---\nname: secret\n---\n# S\n\nSECRET").unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("leak.md")).unwrap();

        let skills = load_skills_from_dirs(std::slice::from_ref(&dir));
        assert!(skills.is_empty(), "symlinked skill files must be rejected");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&outside);
    }
}
