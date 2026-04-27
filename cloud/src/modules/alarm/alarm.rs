use serde::{Deserialize, Serialize};

/// 批量操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOperationResult {
    pub success_count: usize,
    pub total_count: usize,
}
