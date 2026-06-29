//! Patrol types — trust configuration, wake priority, wake signals.
//! Will be populated in Task 3.

/// Trust configuration for an agent.
#[derive(Debug, Clone)]
pub struct TrustConfig {
    pub level: TrustLevel,
}

/// Trust level for agent actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustLevel {
    Low,
    Medium,
    High,
}

/// Priority for waking an agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakePriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Signal to wake an agent.
#[derive(Debug, Clone)]
pub struct WakeSignal {
    pub priority: WakePriority,
    pub reason: String,
}
