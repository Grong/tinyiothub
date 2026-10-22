//! E4d 地图空间模型：几何 / zone 解析 / trust 派生。
//!
//! 纯函数层，无 db。持久化经 `ports` 注入（后续 Task）。

pub mod model;
pub mod zone;

pub use model::{
    Geometry, LocationTrust, MapBounds, MapRecord, PlaceKind, PlaceRecord, PlacementMode, ThingPosition,
    TrustThresholds,
};
pub use zone::{derive_trust, point_in_polygon, polygon_area, resolve_zone_for, resolve_zone_in};
