//! Turn agenda discovery.
//!
//! The brief names the skills for this turn; the skill files provide the
//! executable steps. This module only turns the brief into stable agenda keys
//! and labels. It does not execute skills or mutate the todo list.

use tinyiothub_agent::harness::{AgendaItem, AgendaStatus};
use tinyiothub_skills::LoadedSkill;

use crate::domains::agent::pool::load_workspace_skills;
use crate::domains::agent::prompt::paths;

/// Extract skills referenced by the brief, in brief order.
fn is_name_char(value: u8) -> bool {
    value.is_ascii_alphanumeric() || value == b'_' || value == b'-'
}

/// Find a canonical skill name without matching it inside another skill name
/// (`inspection` must not match the `-inspection` suffix of `video-inspection`).
fn find_name(brief: &str, name: &str) -> Option<usize> {
    let bytes = brief.as_bytes();
    let mut start = 0;
    while let Some(offset) = brief[start..].find(name) {
        let position = start + offset;
        let before = position.checked_sub(1).map(|index| bytes[index]);
        let after = bytes.get(position + name.len()).copied();
        let before_ok = before.is_none_or(|value| !is_name_char(value));
        let after_ok = after.is_none_or(|value| !is_name_char(value));
        if before_ok && after_ok {
            return Some(position);
        }
        start = position + name.len();
    }
    None
}

fn find_skill(brief: &str, skill: &LoadedSkill) -> Option<usize> {
    // Prefer the canonical id (what TURN_BRIEF normally uses), then fall back
    // to the display title so Chinese briefs still resolve.
    find_name(brief, &skill.name).or_else(|| brief.find(&skill.title))
}

pub fn parse_turn_brief(brief: &str, skills: &[LoadedSkill]) -> Vec<AgendaItem> {
    let mut found = skills
        .iter()
        .filter_map(|skill| {
            find_skill(brief, skill).map(|position| {
                (
                    position,
                    AgendaItem {
                        key: skill.name.clone(),
                        label: skill.title.clone(),
                        status: AgendaStatus::Pending,
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    found.sort_by_key(|(position, _)| *position);
    found.dedup_by(|(_, left), (_, right)| left.key == right.key);
    found.into_iter().map(|(_, item)| item).collect()
}

/// Read the workspace brief and resolve its skill agenda.
pub async fn load_turn_agenda(workspace_id: &str) -> Option<Vec<AgendaItem>> {
    let directory = paths::workspace_dir(workspace_id);
    let brief = tokio::fs::read_to_string(directory.join("TURN_BRIEF.md")).await.ok()?;
    let skills = load_workspace_skills(&directory);
    let agenda = parse_turn_brief(&brief, &skills);
    (!agenda.is_empty()).then_some(agenda)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_skills::InjectMode;

    fn skill(name: &str, title: &str) -> LoadedSkill {
        LoadedSkill {
            name: name.into(),
            title: title.into(),
            description: String::new(),
            version: "1".into(),
            inject: InjectMode::Index,
            trigger: None,
            body: String::new(),
        }
    }

    #[test]
    fn brief_order_is_preserved_and_unknown_skills_are_ignored() {
        let skills = [skill("inspection", "巡检"), skill("energy-saving", "节能")];
        let agenda = parse_turn_brief("先做节能，再做巡检；不要加载 unknown。", &skills);
        assert_eq!(
            agenda
                .iter()
                .map(|item| (item.key.as_str(), item.label.as_str()))
                .collect::<Vec<_>>(),
            vec![("energy-saving", "节能"), ("inspection", "巡检")]
        );
        assert!(agenda.iter().all(|item| item.status == AgendaStatus::Pending));
    }

    #[test]
    fn video_inspection_name_does_not_collide_with_inspection() {
        let skills = [skill("inspection", "巡检"), skill("video-inspection", "视频巡检")];
        let agenda = parse_turn_brief(
            "1. 使用 `inspection` 完成设备巡检。2. 使用 `video-inspection` 做视频巡检。",
            &skills,
        );
        assert_eq!(
            agenda.iter().map(|item| item.key.as_str()).collect::<Vec<_>>(),
            vec!["inspection", "video-inspection"]
        );
    }

    #[test]
    fn duplicate_mentions_are_deduplicated() {
        let skills = [skill("inspection", "巡检")];
        let agenda = parse_turn_brief("巡检先做巡检", &skills);
        assert_eq!(agenda.len(), 1);
    }
}
