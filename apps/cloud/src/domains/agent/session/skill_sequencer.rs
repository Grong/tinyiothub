//! 一轮里的技能执行顺序。
//!
//! Harness 要求「一次只执行一个技能」：加载 → 执行 → `complete_skill` 结算 → 下一个。
//! 顺序来自 TURN_BRIEF（见 `agenda.rs`）；本模块只负责状态机，不读文件、不发事件。
//!
//! 三个保证：
//! 1. 已有技能未结算时，加载另一个技能会被拒绝（`request` 返回当前 active）。
//! 2. `finish` 自动把下一个待执行技能设为 active，调用方据此把技能正文直接回灌给模型。
//! 3. 失败/阻塞/跳过都算结算，自动继续下一个，不会卡住整轮。

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    pub key: String,
    pub title: String,
}

impl SkillEntry {
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillOutcome {
    Completed,
    Skipped,
    Blocked,
}

impl SkillOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Skipped => "skipped",
            Self::Blocked => "blocked",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "skipped" | "skip" => Self::Skipped,
            "blocked" | "block" => Self::Blocked,
            _ => Self::Completed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FinishedSkill {
    pub entry: SkillEntry,
    pub outcome: SkillOutcome,
    pub summary: String,
}

#[derive(Debug, Clone, Default)]
pub struct FinishResult {
    pub finished: Option<FinishedSkill>,
    pub next: Option<SkillEntry>,
}

#[derive(Debug, Default)]
struct TurnSkills {
    order: Vec<SkillEntry>,
    active: Option<SkillEntry>,
    finished: Vec<FinishedSkill>,
}

impl TurnSkills {
    fn next_pending(&self) -> Option<SkillEntry> {
        let active = self.active.as_ref().map(|entry| &entry.key);
        self.order
            .iter()
            .find(|entry| Some(&entry.key) != active && !self.finished.iter().any(|done| done.entry.key == entry.key))
            .cloned()
    }

    fn is_finished(&self, key: &str) -> bool {
        self.finished.iter().any(|done| done.entry.key == key)
    }
}

/// 按 session 保存的技能状态机。进程内共享，工具层和 runner 都拿同一个实例。
///
/// 状态是「每轮」的：runner 在 turn 开始重置、结束清理。进程重启会中断该轮，
/// 新轮次从 TURN_BRIEF 重新开始，所以这里不做持久化。
#[derive(Debug, Default)]
pub struct SkillSequencer {
    turns: Mutex<HashMap<String, TurnSkills>>,
}

impl SkillSequencer {
    pub fn new() -> Self {
        Self::default()
    }

    fn with_turn<T>(&self, session_key: &str, f: impl FnOnce(&mut TurnSkills) -> T) -> T {
        let mut guard = self.turns.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = guard.entry(session_key.to_string()).or_default();
        f(entry)
    }

    /// 轮次开始：写入 TURN_BRIEF 的技能顺序，清空上一轮状态。
    pub fn begin(&self, session_key: &str, order: Vec<SkillEntry>) {
        if session_key.is_empty() {
            return;
        }
        self.with_turn(session_key, |turn| {
            *turn = TurnSkills {
                order,
                ..TurnSkills::default()
            };
        });
    }

    /// 轮次结束：释放该 session 的状态。
    pub fn end(&self, session_key: &str) {
        let mut guard = self.turns.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.remove(session_key);
    }

    pub fn active(&self, session_key: &str) -> Option<SkillEntry> {
        if session_key.is_empty() {
            return None;
        }
        self.turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(session_key)
            .and_then(|turn| turn.active.clone())
    }

    pub fn pending(&self, session_key: &str) -> Vec<SkillEntry> {
        self.turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(session_key)
            .map(|turn| {
                turn.order
                    .iter()
                    .filter(|entry| {
                        Some(&entry.key) != turn.active.as_ref().map(|active| &active.key)
                            && !turn.is_finished(&entry.key)
                    })
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 申请加载一个技能。已有别的技能在跑 → 返回 Err(当前技能)。
    /// 空闲时允许加载任意技能：TURN_BRIEF 的顺序只是建议，硬性保证只有“一次一个”。
    pub fn request(&self, session_key: &str, key: &str, title: &str) -> Result<(), SkillEntry> {
        if session_key.is_empty() {
            return Ok(());
        }
        self.with_turn(session_key, |turn| {
            if let Some(active) = &turn.active {
                if active.key == key {
                    return Ok(());
                }
                return Err(active.clone());
            }
            let entry = turn
                .order
                .iter()
                .find(|item| item.key == key)
                .cloned()
                .unwrap_or_else(|| SkillEntry::new(key, title));
            turn.active = Some(SkillEntry {
                key: entry.key.clone(),
                title: if entry.title.is_empty() {
                    title.to_string()
                } else {
                    entry.title
                },
            });
            Ok(())
        })
    }

    /// 加载失败时回滚，避免把不存在的技能锁成 active。
    pub fn cancel(&self, session_key: &str, key: &str) {
        if session_key.is_empty() {
            return;
        }
        self.with_turn(session_key, |turn| {
            if turn.active.as_ref().is_some_and(|active| active.key == key) {
                turn.active = None;
            }
        });
    }

    /// 结算当前技能，并自动把下一个待执行技能设为 active。
    pub fn finish(&self, session_key: &str, outcome: SkillOutcome, summary: &str) -> FinishResult {
        if session_key.is_empty() {
            return FinishResult::default();
        }
        self.with_turn(session_key, |turn| {
            // 没有正在执行的技能时只做无操作结算，绝不能顺手把第一个技能「启动」了：
            // 新轮次开头模型常误调 complete_skill，那会把巡检凭空加载一次。
            let Some(entry) = turn.active.take() else {
                return FinishResult::default();
            };
            let finished = Some(FinishedSkill {
                entry,
                outcome,
                summary: summary.to_string(),
            });
            if let Some(done) = &finished {
                turn.finished.push(done.clone());
            }
            let next = turn.next_pending();
            turn.active = next.clone();
            FinishResult { finished, next }
        })
    }

    /// 最近一次结算的技能（runner 用它把 agenda 标完成/跳过/阻塞）。
    pub fn last_finished(&self, session_key: &str) -> Option<FinishedSkill> {
        self.finished(session_key).last().cloned()
    }

    /// 当前轮次里已经结算的技能 key（测试与观测用）。
    pub fn finished(&self, session_key: &str) -> Vec<FinishedSkill> {
        self.turns
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(session_key)
            .map(|turn| turn.finished.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> Vec<SkillEntry> {
        vec![
            SkillEntry::new("inspection", "巡检"),
            SkillEntry::new("video-inspection", "视频巡检"),
            SkillEntry::new("alarm-management", "告警管理"),
        ]
    }

    #[test]
    fn one_skill_at_a_time() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        assert!(seq.request("s", "inspection", "巡检").is_ok());
        // 第二个技能必须等结算
        let err = seq.request("s", "video-inspection", "视频巡检").unwrap_err();
        assert_eq!(err.key, "inspection");
        // 重复加载同一个技能允许
        assert!(seq.request("s", "inspection", "巡检").is_ok());
    }

    #[test]
    fn finish_advances_automatically() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        seq.request("s", "inspection", "巡检").unwrap();
        let result = seq.finish("s", SkillOutcome::Completed, "12 台设备正常");
        assert_eq!(result.finished.unwrap().entry.key, "inspection");
        assert_eq!(result.next.unwrap().key, "video-inspection");
        // 下一个已经是 active，可以直接执行，不需要再 get_skill
        assert_eq!(seq.active("s").unwrap().key, "video-inspection");
        assert!(seq.request("s", "video-inspection", "视频巡检").is_ok());
    }

    #[test]
    fn blocked_and_skipped_continue_to_next() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        seq.request("s", "inspection", "巡检").unwrap();
        let skipped = seq.finish("s", SkillOutcome::Blocked, "逆变器无响应");
        assert_eq!(skipped.next.unwrap().key, "video-inspection");
        let blocked = seq.finish("s", SkillOutcome::Skipped, "跳过");
        assert_eq!(blocked.next.unwrap().key, "alarm-management");
        let last = seq.finish("s", SkillOutcome::Completed, "完成");
        assert!(last.next.is_none());
        assert_eq!(seq.pending("s").len(), 0);
        assert_eq!(seq.finished("s").len(), 3);
    }

    #[test]
    fn out_of_order_load_is_allowed_when_idle() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        assert!(seq.request("s", "alarm-management", "告警管理").is_ok());
        // 但只要有一个在跑，另一个仍然要被拒绝
        let err = seq.request("s", "inspection", "巡检").unwrap_err();
        assert_eq!(err.key, "alarm-management");
    }

    #[test]
    fn ad_hoc_skill_outside_brief_is_allowed() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        assert!(seq.request("s", "troubleshooting", "故障排查").is_ok());
    }

    #[test]
    fn finish_without_active_is_a_no_op() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        let result = seq.finish("s", SkillOutcome::Completed, "");
        assert!(result.finished.is_none());
        assert!(result.next.is_none());
        assert!(seq.active("s").is_none());
    }

    #[test]
    fn failed_load_can_be_cancelled() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        seq.request("s", "inspection", "巡检").unwrap();
        seq.cancel("s", "inspection");
        assert!(seq.active("s").is_none());
        assert!(seq.request("s", "inspection", "巡检").is_ok());
    }

    #[test]
    fn end_clears_state() {
        let seq = SkillSequencer::new();
        seq.begin("s", order());
        seq.request("s", "inspection", "巡检").unwrap();
        seq.end("s");
        assert!(seq.active("s").is_none());
        assert!(seq.pending("s").is_empty());
    }
}
