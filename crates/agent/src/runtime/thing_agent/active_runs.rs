//! In-memory registry of in-flight runs (AI run transparency T3) — the data
//! source for live progress polling. Bounded; registration beyond capacity is
//! refused with a warn metric. Deregistration is RAII via [`ActiveRunGuard`].

use std::sync::Arc;

use dashmap::DashMap;

use crate::runtime::thing_agent::manager::ACTIVE_RUNS_CAPACITY;
use crate::runtime::thing_agent::runner::RunContext;

/// Registry of currently-running runs, keyed by `run_id`. Cheap to clone —
/// all clones share the same map.
#[derive(Clone)]
pub struct ActiveRunRegistry {
    runs: Arc<DashMap<String, RunContext>>,
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
    pub fn register(&self, ctx: RunContext) -> Option<ActiveRunGuard> {
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
        self.runs.insert(run_id.clone(), ctx);
        Some(ActiveRunGuard {
            runs: Arc::clone(&self.runs),
            run_id,
        })
    }

    pub fn get(&self, run_id: &str) -> Option<RunContext> {
        self.runs.get(run_id).map(|r| r.clone())
    }
}

/// RAII deregistration: dropping removes the run from the registry, so the
/// cancel path needs no explicit unregister call.
pub struct ActiveRunGuard {
    runs: Arc<DashMap<String, RunContext>>,
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
    use crate::runtime::thing_agent::runner::RunContext;

    #[tokio::test]
    async fn register_visible_then_guard_drop_removes() {
        let reg = ActiveRunRegistry::new(2);
        let ctx = RunContext::new("r1".into(), "ws1".into(), "t".into());
        {
            let _g = reg.register(ctx.clone()).expect("容量内");
            assert!(reg.get("r1").is_some());
        } // guard drop
        assert!(reg.get("r1").is_none());
    }

    #[tokio::test]
    async fn over_capacity_refuses_with_warn() {
        let reg = ActiveRunRegistry::new(1);
        let _g1 = reg.register(RunContext::new("r1".into(), "ws".into(), "t".into()));
        assert!(
            reg.register(RunContext::new("r2".into(), "ws".into(), "t".into()))
                .is_none()
        );
    }
}
