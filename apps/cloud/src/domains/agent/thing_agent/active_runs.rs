//! In-memory registry of in-flight runs (AI run transparency T3) — the data
//! source for live progress polling. Bounded; registration beyond capacity is
//! refused with a warn metric. Deregistration is RAII via [`ActiveRunGuard`].

use std::sync::Arc;

use dashmap::DashMap;
use tokio_util::sync::CancellationToken;

use crate::domains::agent::thing_agent::manager::ACTIVE_RUNS_CAPACITY;
use crate::domains::agent::thing_agent::runner::RunContext;

#[derive(Clone)]
pub struct ActiveRun {
    pub ctx: RunContext,
    pub cancel: CancellationToken,
}

/// Registry of currently-running runs, keyed by `run_id`. Cheap to clone —
/// all clones share the same map.
#[derive(Clone)]
pub struct ActiveRunRegistry {
    runs: Arc<DashMap<String, ActiveRun>>,
    max: usize,
}

impl ActiveRunRegistry {
    /// Production constructor: capacity tied to scheduler concurrency capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(ACTIVE_RUNS_CAPACITY)
    }

    pub fn new(max: usize) -> Self {
        Self {
            runs: Arc::new(DashMap::new()),
            max,
        }
    }

    /// Register a run; returns the RAII guard whose drop deregisters it.
    /// At capacity the registration is refused (`None`) with a warn metric.
    pub fn register(&self, ctx: RunContext, cancel: CancellationToken) -> Option<ActiveRunGuard> {
        if self.runs.len() >= self.max {
            tracing::warn!(
                metric = "active_run_registry_full",
                run_id = %ctx.run_id,
                max = self.max,
                "active run registry full — registration refused"
            );
            return None;
        }
        let run_id = ctx.run_id.clone();
        self.runs.insert(run_id.clone(), ActiveRun { ctx, cancel });
        Some(ActiveRunGuard {
            runs: Arc::clone(&self.runs),
            run_id,
        })
    }

    pub fn get(&self, run_id: &str) -> Option<RunContext> {
        self.runs.get(run_id).map(|r| r.ctx.clone())
    }

    pub fn cancel(&self, run_id: &str) -> bool {
        self.runs.get(run_id).map(|run| run.cancel.cancel()).is_some()
    }

    pub fn steer(&self, run_id: &str, facts: String) -> bool {
        self.runs
            .get(run_id)
            .map(|run| {
                run.ctx
                    .steering
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .push(facts);
            })
            .is_some()
    }
}

/// RAII deregistration: dropping removes the run from the registry, so the
/// cancel path needs no explicit unregister call.
pub struct ActiveRunGuard {
    runs: Arc<DashMap<String, ActiveRun>>,
    run_id: String,
}

impl Drop for ActiveRunGuard {
    fn drop(&mut self) {
        self.runs.remove(&self.run_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    use crate::domains::agent::thing_agent::runner::RunContext;

    #[tokio::test]
    async fn register_visible_then_guard_drop_removes() {
        let reg = ActiveRunRegistry::new(2);
        let ctx = RunContext::new("r1".into(), "ws1".into(), "t".into());
        {
            let _g = reg.register(ctx.clone(), CancellationToken::new()).expect("容量内");
            assert!(reg.get("r1").is_some());
        } // guard drop
        assert!(reg.get("r1").is_none());
    }

    #[tokio::test]
    async fn cancel_uses_the_callers_shared_token() {
        let reg = ActiveRunRegistry::new(1);
        let cancel = CancellationToken::new();
        let _guard = reg
            .register(RunContext::new("r1".into(), "ws".into(), "t".into()), cancel.clone())
            .expect("register");
        assert!(reg.cancel("r1"));
        assert!(cancel.is_cancelled());
        assert!(!reg.cancel("missing"));
    }

    #[tokio::test]
    async fn over_capacity_refuses_with_warn() {
        let reg = ActiveRunRegistry::new(1);
        let _g1 = reg.register(
            RunContext::new("r1".into(), "ws".into(), "t".into()),
            CancellationToken::new(),
        );
        assert!(
            reg.register(
                RunContext::new("r2".into(), "ws".into(), "t".into()),
                CancellationToken::new()
            )
            .is_none()
        );
    }
}
