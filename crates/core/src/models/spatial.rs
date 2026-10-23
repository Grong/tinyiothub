//! E4d 空间模型值类型：地图、地点、坐标与 trust。
//!
//! 只含共享 DTO（core 禁逻辑）：持久化在 `db::map`，几何/zone 纯函数在
//! runtime（Task 2）。坐标单位为米，地图帧为工作区本地 2D 平面。

use serde::{Deserialize, Serialize};

/// 地图边界（工作区本地 2D 平面，单位米）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MapBounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 地点类型：地标 / 区域（zone 仅 Polygon）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    Landmark,
    Zone,
}

impl PlaceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Landmark => "landmark",
            Self::Zone => "zone",
        }
    }

    /// 未知值按 landmark 处理（兼容前向新增类型，不阻塞读取）。
    pub fn from_db(s: &str) -> Self {
        match s {
            "zone" => Self::Zone,
            _ => Self::Landmark,
        }
    }
}

/// 地点几何：点或闭合多边形（≥3 点）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Geometry {
    Point { x: f64, y: f64 },
    Polygon { points: Vec<(f64, f64)> },
}

/// 地图记录（持久化 DTO）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MapRecord {
    pub id: String,
    pub workspace_id: String,
    pub name: String,
    pub bounds: MapBounds,
    pub anchor_map_id: Option<String>,
    pub is_default: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// 地点记录（持久化 DTO）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PlaceRecord {
    pub id: String,
    pub map_id: String,
    pub name: String,
    pub kind: PlaceKind,
    pub geometry: Geometry,
    pub heading: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
}

/// 位置可信度（runtime 按 age 派生；驱动上报仅作上限）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocationTrust {
    Trusted,
    #[default]
    Uncertain,
    Lost,
}

impl LocationTrust {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trusted => "trusted",
            Self::Uncertain => "uncertain",
            Self::Lost => "lost",
        }
    }

    /// 未知/缺失值按 uncertain 处理（保守：不冒充 trusted）。
    pub fn from_db(s: Option<&str>) -> Self {
        match s {
            Some("trusted") => Self::Trusted,
            Some("lost") => Self::Lost,
            _ => Self::Uncertain,
        }
    }
}

/// 物坐标（things 坐标列；x/y 缺失表示未定位）。
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ThingPosition {
    pub map_id: Option<String>,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub level: Option<i64>,
    pub heading: Option<f64>,
    #[serde(default)]
    pub trust: LocationTrust,
    pub updated_at: Option<String>,
}
