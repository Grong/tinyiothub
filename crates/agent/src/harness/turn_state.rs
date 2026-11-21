//! Turn/agenda state machine.
//!
//! This is the control-plane shape shared by the runner and its projections.
//! It deliberately contains no I/O, no model calls, and no tool execution.

use serde::{Deserialize, Serialize};

/// Lifecycle phase of one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnPhase {
    Planning,
    Executing,
    Verifying,
    Finalizing,
    Completed,
    Failed,
    Cancelled,
}

/// Progress of one agenda entry (a skill or explicit work package).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgendaStatus {
    Pending,
    InProgress,
    Completed,
    Skipped,
    Blocked,
}

impl AgendaStatus {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Skipped | Self::Blocked)
    }
}

/// One entry in the turn agenda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgendaItem {
    pub key: String,
    pub label: String,
    pub status: AgendaStatus,
}

/// Why a turn cannot finalize yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalizeBlocker {
    NoAgenda,
    NoActiveItem,
    PendingItems(Vec<String>),
    Phase(TurnPhase),
}

/// Pure, replayable state for one turn. The runner owns transitions; consumers
/// may project it into a session snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnState {
    phase: TurnPhase,
    agenda: Vec<AgendaItem>,
    steps: u32,
    tool_calls: u32,
}

impl Default for TurnState {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnState {
    pub fn new() -> Self {
        Self {
            phase: TurnPhase::Planning,
            agenda: Vec::new(),
            steps: 0,
            tool_calls: 0,
        }
    }

    pub fn with_agenda(items: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) -> Self {
        let mut state = Self::new();
        state.set_agenda(items);
        state
    }

    pub fn phase(&self) -> TurnPhase {
        self.phase
    }

    pub fn agenda(&self) -> &[AgendaItem] {
        &self.agenda
    }

    pub fn steps(&self) -> u32 {
        self.steps
    }

    pub fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub fn set_agenda(&mut self, items: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>) {
        self.agenda = items
            .into_iter()
            .map(|(key, label)| AgendaItem {
                key: key.into(),
                label: label.into(),
                status: AgendaStatus::Pending,
            })
            .collect();
        self.phase = TurnPhase::Planning;
    }

    /// Mark one agenda item active. Only one item may be active at a time.
    pub fn start(&mut self, key: &str) -> Result<(), FinalizeBlocker> {
        let Some(index) = self.index_of(key) else {
            return Err(FinalizeBlocker::NoAgenda);
        };
        if self.agenda[index].status.is_terminal() {
            return Err(FinalizeBlocker::Phase(self.phase));
        }
        for item in &mut self.agenda {
            if item.status == AgendaStatus::InProgress {
                return Err(FinalizeBlocker::NoActiveItem);
            }
        }
        self.agenda[index].status = AgendaStatus::InProgress;
        self.phase = TurnPhase::Executing;
        Ok(())
    }

    /// Resolve the active item and finish. `Completed`/`Skipped`/`Blocked` are
    /// all terminal; the caller records the reason separately for skipped and
    /// blocked items.
    pub fn resolve(&mut self, key: &str, status: AgendaStatus) -> Result<(), FinalizeBlocker> {
        if !status.is_terminal() {
            return Err(FinalizeBlocker::Phase(self.phase));
        }
        let Some(index) = self.index_of(key) else {
            return Err(FinalizeBlocker::NoAgenda);
        };
        if self.agenda[index].status != AgendaStatus::InProgress {
            return Err(FinalizeBlocker::NoActiveItem);
        }
        self.agenda[index].status = status;
        self.phase = TurnPhase::Executing;
        Ok(())
    }

    pub fn note_step(&mut self) {
        self.steps = self.steps.saturating_add(1);
    }

    pub fn note_tool_call(&mut self) {
        self.tool_calls = self.tool_calls.saturating_add(1);
    }

    /// Gate before a completed final answer is emitted.
    pub fn can_finalize(&self) -> Result<(), FinalizeBlocker> {
        if self.agenda.is_empty() {
            return Err(FinalizeBlocker::NoAgenda);
        }
        if self.agenda.iter().any(|item| item.status == AgendaStatus::InProgress) {
            return Err(FinalizeBlocker::NoActiveItem);
        }
        let pending = self
            .agenda
            .iter()
            .filter(|item| !item.status.is_terminal())
            .map(|item| item.key.clone())
            .collect::<Vec<_>>();
        if !pending.is_empty() {
            return Err(FinalizeBlocker::PendingItems(pending));
        }
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), FinalizeBlocker> {
        self.can_finalize()?;
        self.phase = TurnPhase::Finalizing;
        self.phase = TurnPhase::Completed;
        Ok(())
    }

    pub fn fail(&mut self) {
        self.phase = TurnPhase::Failed;
    }

    pub fn cancel(&mut self) {
        self.phase = TurnPhase::Cancelled;
    }

    fn index_of(&self, key: &str) -> Option<usize> {
        self.agenda.iter().position(|item| item.key == key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_agenda_items_must_be_terminal_before_finalize() {
        let mut state = TurnState::with_agenda([("inspection", "巡检"), ("energy", "节能")]);
        state.start("inspection").unwrap();
        state.resolve("inspection", AgendaStatus::Completed).unwrap();
        assert_eq!(
            state.can_finalize(),
            Err(FinalizeBlocker::PendingItems(vec!["energy".into()]))
        );

        state.start("energy").unwrap();
        state.resolve("energy", AgendaStatus::Skipped).unwrap();
        assert!(state.can_finalize().is_ok());
        state.finish().unwrap();
        assert_eq!(state.phase(), TurnPhase::Completed);
    }

    #[test]
    fn only_one_agenda_item_may_be_active() {
        let mut state = TurnState::with_agenda([("a", "A"), ("b", "B")]);
        state.start("a").unwrap();
        assert_eq!(state.start("b"), Err(FinalizeBlocker::NoActiveItem));
    }

    #[test]
    fn active_item_blocks_finalize() {
        let mut state = TurnState::with_agenda([("a", "A")]);
        state.start("a").unwrap();
        assert_eq!(state.can_finalize(), Err(FinalizeBlocker::NoActiveItem));
    }

    #[test]
    fn empty_agenda_cannot_finalize() {
        assert_eq!(TurnState::new().can_finalize(), Err(FinalizeBlocker::NoAgenda));
    }
}
