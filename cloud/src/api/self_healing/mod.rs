// Self-Healing API Module
// HTTP endpoint handlers for self-healing management

mod handlers;

pub use handlers::create_router;

use std::sync::Arc;
use std::sync::OnceLock;

use tokio::sync::RwLock;

use crate::domain::self_healing::{ActionExecutor, PolicyEvaluator, SelfHealingPolicy, HealingExecutionRepository};
use tinyiothub_core::models::self_healing::ProbeConfig;
use crate::infrastructure::persistence::database::Database;
use crate::infrastructure::self_healing::ProbeScheduler;

/// Global self-healing state
static SELF_HEALING_STATE: OnceLock<Arc<RwLock<SelfHealingState>>> = OnceLock::new();

/// Self-healing runtime state
pub struct SelfHealingState {
    pub policy: SelfHealingPolicy,
    pub evaluator: Arc<PolicyEvaluator>,
    pub executor: Arc<ActionExecutor>,
    pub repository: Arc<HealingExecutionRepository>,
    pub scheduler: Arc<ProbeScheduler>,
    pub probe_config: ProbeConfig,
}

impl SelfHealingState {
    pub fn new(db: Arc<Database>) -> Self {
        let policy = SelfHealingPolicy::default();
        let evaluator = Arc::new(PolicyEvaluator::new(policy.clone()));
        let repository = Arc::new(HealingExecutionRepository::new(db));
        let probe_config = ProbeConfig::default();
        // Create a separate PolicyEvaluator for the scheduler since it takes ownership
        let scheduler_evaluator = PolicyEvaluator::new(policy.clone());
        let scheduler = Arc::new(ProbeScheduler::new(
            probe_config.clone(),
            scheduler_evaluator,
            tokio::sync::broadcast::channel::<()>(1).1, // dummy receiver; replaced at spawn
        ));
        Self {
            policy,
            evaluator,
            executor: Arc::new(ActionExecutor::new()),
            repository,
            scheduler,
            probe_config,
        }
    }
}

/// Initialize global self-healing state (call once at startup)
pub fn init_self_healing_state(db: Arc<Database>) -> Arc<RwLock<SelfHealingState>> {
    SELF_HEALING_STATE
        .get_or_init(|| Arc::new(RwLock::new(SelfHealingState::new(db))))
        .clone()
}

/// Get self-healing state
pub fn get_self_healing_state() -> Option<Arc<RwLock<SelfHealingState>>> {
    SELF_HEALING_STATE.get().cloned()
}
