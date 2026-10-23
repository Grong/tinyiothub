//! E4d 空间模型 runtime 视图。
//!
//! 共享 DTO 由 core 定义（`core::models::spatial`），这里只做再导出并补充
//! 纯函数所需的参数类型（trust 阈值）。runtime 层不引入 db。

pub use tinyiothub_core::driver::manifest::PlacementMode;
pub use tinyiothub_core::models::spatial::{
    Geometry, LocationTrust, MapBounds, MapRecord, PlaceKind, PlaceRecord, ThingPosition,
};

use chrono::Duration;

/// trust 派生阈值（workspace 可配）。
///
/// `age <= trusted_max` → Trusted；`age <= uncertain_max` → Uncertain；
/// 更旧 → Lost。默认 30s / 5min（spec §0 裁定 #5）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrustThresholds {
    pub trusted_max: Duration,
    pub uncertain_max: Duration,
}

impl TrustThresholds {
    pub fn new(trusted_max: Duration, uncertain_max: Duration) -> Self {
        Self {
            trusted_max,
            uncertain_max,
        }
    }
}

impl Default for TrustThresholds {
    fn default() -> Self {
        Self {
            trusted_max: Duration::seconds(30),
            uncertain_max: Duration::minutes(5),
        }
    }
}
