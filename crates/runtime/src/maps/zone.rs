//! E4d 几何 / zone 解析 / trust 派生纯函数（无 db）。
//!
//! 语义见 `docs/superpowers/specs/2026-10-22-e4d-spatial-model-design.md` §2.2：
//! - point-in-polygon 边界含入（边/顶点视为区域内）；
//! - 命中多个 zone 取面积最小，面积相同按 name、id 确定性排序；
//! - 显式 `placement.zoneRef` 优先；未知显式引用 fail-closed（不回落点选）；
//! - trust 由 age 派生，驱动上报只能降级。

use chrono::{DateTime, Utc};

use super::model::{Geometry, LocationTrust, PlaceKind, PlaceRecord, PlacementMode, TrustThresholds};

/// 判断点是否在多边形内（射线法），边界/顶点含入。
///
/// 少于 3 点或零面积（退化）多边形一律 `false`。
pub fn point_in_polygon(point: (f64, f64), polygon: &[(f64, f64)]) -> bool {
    if polygon.len() < 3 || polygon_area(polygon) == 0.0 {
        return false;
    }
    if polygon
        .iter()
        .enumerate()
        .any(|(i, &a)| point_on_segment(point, a, polygon[(i + 1) % polygon.len()]))
    {
        return true;
    }

    // 水平向右射线与边的交点计数；半开区间规则避免顶点重复计数。
    let (px, py) = point;
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if (yi > py) != (yj > py) {
            let x_intersect = (xj - xi) * (py - yi) / (yj - yi) + xi;
            if px < x_intersect {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// 鞋带公式面积（绝对值），少于 3 点返回 0。
pub fn polygon_area(polygon: &[(f64, f64)]) -> f64 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..polygon.len() {
        let (x1, y1) = polygon[i];
        let (x2, y2) = polygon[(i + 1) % polygon.len()];
        sum += x1 * y2 - x2 * y1;
    }
    (sum / 2.0).abs()
}

/// 点落在 zone 多边形内时返回该 zone：仅 `PlaceKind::Zone` 且多边形 ≥3 点，
/// 命中多个取面积最小；面积相同按 `name`、`id` 升序确定性选择。
pub fn resolve_zone_in(places: &[PlaceRecord], point: (f64, f64)) -> Option<&PlaceRecord> {
    places
        .iter()
        .filter(|place| place.kind == PlaceKind::Zone)
        .filter_map(|place| match &place.geometry {
            Geometry::Polygon { points } if points.len() >= 3 => {
                let area = polygon_area(points);
                (area > 0.0 && point_in_polygon(point, points)).then_some((place, area))
            }
            _ => None,
        })
        .min_by(|(place_a, area_a), (place_b, area_b)| {
            area_a
                .total_cmp(area_b)
                .then_with(|| place_a.name.cmp(&place_b.name))
                .then_with(|| place_a.id.cmp(&place_b.id))
        })
        .map(|(place, _)| place)
}

/// zone 解析优先级：显式 `zoneRef` 名命中同批 `PlaceKind::Zone` 即胜出；
/// 否则按 point-in-polygon 选择。
///
/// 显式引用未命中（含名字指向 landmark）返回 `None`，不回落点选——fail-closed，
/// 避免把设备放进错误的 zone。
pub fn resolve_zone_for<'a>(
    places: &'a [PlaceRecord],
    explicit_zone: Option<&str>,
    point: Option<(f64, f64)>,
) -> Option<&'a PlaceRecord> {
    if let Some(name) = explicit_zone {
        return places
            .iter()
            .filter(|place| place.kind == PlaceKind::Zone && place.name == name)
            .min_by(|place_a, place_b| place_a.id.cmp(&place_b.id));
    }
    point.and_then(|point| resolve_zone_in(places, point))
}

/// 按 placement 模式与位置新鲜度派生 trust。
///
/// - `Fixed` / `Declared`（静态位置）→ `Trusted`，不受 age 与驱动上报影响；
/// - `Runtime` → age ≤ `trusted_max` Trusted / ≤ `uncertain_max` Uncertain /
///   更旧 Lost；驱动上报只能降级（取两者更差者）；
/// - 缺失或无法解析时间戳 → `Lost`（没有新鲜位置，不猜测）；
/// - 时间戳在未来按 age=0 处理（视为刚更新）。
pub fn derive_trust(
    placement_mode: PlacementMode,
    position_updated_at: Option<&str>,
    driver_trust: Option<LocationTrust>,
    now: DateTime<Utc>,
    thresholds: TrustThresholds,
) -> LocationTrust {
    if placement_mode != PlacementMode::Runtime {
        // Fixed 恒 trusted；Declared 为绑定期静态位置，同样不随时间失效。
        return LocationTrust::Trusted;
    }

    let derived = match position_updated_at.and_then(parse_timestamp) {
        Some(updated_at) => {
            let age = now.signed_duration_since(updated_at);
            let age = if age < chrono::Duration::zero() {
                chrono::Duration::zero()
            } else {
                age
            };
            if age <= thresholds.trusted_max {
                LocationTrust::Trusted
            } else if age <= thresholds.uncertain_max {
                LocationTrust::Uncertain
            } else {
                LocationTrust::Lost
            }
        }
        None => LocationTrust::Lost,
    };

    match driver_trust {
        Some(reported) => worse(derived, reported),
        None => derived,
    }
}

fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn trust_rank(trust: LocationTrust) -> u8 {
    match trust {
        LocationTrust::Trusted => 0,
        LocationTrust::Uncertain => 1,
        LocationTrust::Lost => 2,
    }
}

fn worse(a: LocationTrust, b: LocationTrust) -> LocationTrust {
    if trust_rank(a) >= trust_rank(b) { a } else { b }
}

fn point_on_segment(point: (f64, f64), a: (f64, f64), b: (f64, f64)) -> bool {
    let cross = (b.0 - a.0) * (point.1 - a.1) - (b.1 - a.1) * (point.0 - a.0);
    if cross != 0.0 {
        return false;
    }
    point.0 >= a.0.min(b.0) && point.0 <= a.0.max(b.0) && point.1 >= a.1.min(b.1) && point.1 <= a.1.max(b.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn place(id: &str, name: &str, kind: PlaceKind, points: &[(f64, f64)]) -> PlaceRecord {
        PlaceRecord {
            id: id.into(),
            map_id: "map-1".into(),
            name: name.into(),
            kind,
            geometry: Geometry::Polygon {
                points: points.to_vec(),
            },
            heading: None,
            created_at: "2026-10-22T00:00:00Z".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        }
    }

    fn zone(id: &str, name: &str, points: &[(f64, f64)]) -> PlaceRecord {
        place(id, name, PlaceKind::Zone, points)
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
    }

    fn timestamp_ago(seconds: i64) -> String {
        (now() - chrono::Duration::seconds(seconds)).to_rfc3339()
    }

    fn square(size: f64) -> Vec<(f64, f64)> {
        vec![(0.0, 0.0), (size, 0.0), (size, size), (0.0, size)]
    }

    #[test]
    fn point_inside_and_outside() {
        let poly = square(2.0);
        assert!(point_in_polygon((1.0, 1.0), &poly));
        assert!(!point_in_polygon((-0.1, 1.0), &poly));
        assert!(!point_in_polygon((2.1, 1.0), &poly));
        assert!(!point_in_polygon((1.0, 2.1), &poly));
    }

    #[test]
    fn point_on_edges_and_vertices_is_inside() {
        let poly = square(2.0);
        // 四条边中点
        assert!(point_in_polygon((1.0, 0.0), &poly));
        assert!(point_in_polygon((2.0, 1.0), &poly));
        assert!(point_in_polygon((1.0, 2.0), &poly));
        assert!(point_in_polygon((0.0, 1.0), &poly));
        // 四个顶点
        for &vertex in &poly {
            assert!(point_in_polygon(vertex, &poly), "vertex {vertex:?}");
        }
        // 但延长线上的点不算
        assert!(!point_in_polygon((3.0, 0.0), &poly));
    }

    #[test]
    fn concave_polygon_notch_is_outside() {
        // L 形：右下缺口
        let poly = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)];
        assert!(point_in_polygon((0.5, 0.5), &poly));
        assert!(point_in_polygon((1.5, 0.5), &poly));
        assert!(!point_in_polygon((1.5, 1.5), &poly));
        // 凹角点在边界上
        assert!(point_in_polygon((1.0, 1.0), &poly));
    }

    #[test]
    fn degenerate_polygons_are_false() {
        assert!(!point_in_polygon((0.0, 0.0), &[]));
        assert!(!point_in_polygon((0.0, 0.0), &[(0.0, 0.0), (1.0, 1.0)]));
        let collinear = vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)];
        assert_eq!(polygon_area(&collinear), 0.0);
        assert!(!point_in_polygon((1.0, 1.0), &collinear));
    }

    #[test]
    fn polygon_area_is_absolute_shoelace() {
        assert!((polygon_area(&square(2.0)) - 4.0).abs() < 1e-9);
        // 反向（顺时针）仍为正
        let mut reversed = square(2.0);
        reversed.reverse();
        assert!((polygon_area(&reversed) - 4.0).abs() < 1e-9);
        assert!((polygon_area(&[(0.0, 0.0), (2.0, 0.0), (0.0, 3.0)]) - 3.0).abs() < 1e-9);
        assert_eq!(polygon_area(&[(0.0, 0.0), (1.0, 1.0)]), 0.0);
    }

    #[test]
    fn smallest_area_zone_wins() {
        let outer = zone("big", "building", &square(10.0));
        let inner = zone("small", "room", &square(2.0));
        let places = vec![outer, inner];
        let hit = resolve_zone_in(&places, (1.0, 1.0)).unwrap();
        assert_eq!(hit.id, "small");

        // 只命中大 zone 时返回大 zone
        let hit = resolve_zone_in(&places, (5.0, 5.0)).unwrap();
        assert_eq!(hit.id, "big");

        // 都不命中
        assert!(resolve_zone_in(&places, (20.0, 20.0)).is_none());
    }

    #[test]
    fn equal_area_ties_break_by_name_then_id() {
        let b = zone("id-b", "beta", &square(2.0));
        let a = zone("id-a", "alpha", &square(2.0));
        // 输入逆序也要确定性
        let places = vec![b.clone(), a.clone()];
        assert_eq!(resolve_zone_in(&places, (1.0, 1.0)).unwrap().id, "id-a");
        assert_eq!(resolve_zone_in(&places, (1.0, 1.0)).unwrap().name, "alpha");

        // 同名时按 id
        let z2 = zone("id-2", "same", &square(2.0));
        let z1 = zone("id-1", "same", &square(2.0));
        let places = vec![z2, z1];
        assert_eq!(resolve_zone_in(&places, (1.0, 1.0)).unwrap().id, "id-1");
    }

    #[test]
    fn landmarks_are_ignored_by_point_resolution() {
        let landmark = place("lm", "door", PlaceKind::Landmark, &square(2.0));
        assert!(resolve_zone_in(&[landmark], (1.0, 1.0)).is_none());
    }

    #[test]
    fn explicit_zone_ref_takes_precedence() {
        let a = zone("a", "zone-a", &square(10.0));
        let b = zone("b", "zone-b", &square(2.0));
        let places = vec![a, b];

        // 点在 zone-b，但显式引用 zone-a
        let hit = resolve_zone_for(&places, Some("zone-a"), Some((1.0, 1.0))).unwrap();
        assert_eq!(hit.id, "a");

        // 无显式引用时按点选（面积最小）
        assert_eq!(resolve_zone_for(&places, None, Some((1.0, 1.0))).unwrap().id, "b");
    }

    #[test]
    fn unknown_explicit_zone_ref_fails_closed() {
        let b = zone("b", "zone-b", &square(2.0));
        let places = vec![b];
        // 名字未知：不回落到点选
        assert!(resolve_zone_for(&places, Some("missing"), Some((1.0, 1.0))).is_none());
        // 指向 landmark：同样不命中
        let landmark = place("lm", "door", PlaceKind::Landmark, &square(2.0));
        assert!(resolve_zone_for(&[landmark], Some("door"), Some((1.0, 1.0))).is_none());
    }

    #[test]
    fn no_explicit_ref_and_no_point_yields_none() {
        let places = vec![zone("b", "zone-b", &square(2.0))];
        assert!(resolve_zone_for(&places, None, None).is_none());
    }

    #[test]
    fn fixed_placement_is_always_trusted() {
        assert_eq!(
            derive_trust(
                PlacementMode::Fixed,
                Some(&timestamp_ago(3600)),
                Some(LocationTrust::Lost),
                now(),
                TrustThresholds::default(),
            ),
            LocationTrust::Trusted
        );
        // 完全没有时间戳也 trusted（静态位置）
        assert_eq!(
            derive_trust(PlacementMode::Fixed, None, None, now(), TrustThresholds::default(),),
            LocationTrust::Trusted
        );
        // Declared 为绑定期静态位置，同样 trusted
        assert_eq!(
            derive_trust(
                PlacementMode::Declared,
                None,
                Some(LocationTrust::Lost),
                now(),
                TrustThresholds::default(),
            ),
            LocationTrust::Trusted
        );
    }

    #[test]
    fn runtime_trust_thresholds_are_inclusive() {
        let thresholds = TrustThresholds::default();
        let cases = [
            (0, LocationTrust::Trusted),
            (30, LocationTrust::Trusted),
            (31, LocationTrust::Uncertain),
            (300, LocationTrust::Uncertain),
            (301, LocationTrust::Lost),
            (3600, LocationTrust::Lost),
        ];
        for (age, expected) in cases {
            assert_eq!(
                derive_trust(
                    PlacementMode::Runtime,
                    Some(&timestamp_ago(age)),
                    None,
                    now(),
                    thresholds,
                ),
                expected,
                "age={age}"
            );
        }
    }

    #[test]
    fn driver_trust_can_only_downgrade() {
        let thresholds = TrustThresholds::default();
        let fresh = timestamp_ago(0);

        assert_eq!(
            derive_trust(
                PlacementMode::Runtime,
                Some(&fresh),
                Some(LocationTrust::Uncertain),
                now(),
                thresholds,
            ),
            LocationTrust::Uncertain
        );
        assert_eq!(
            derive_trust(
                PlacementMode::Runtime,
                Some(&fresh),
                Some(LocationTrust::Lost),
                now(),
                thresholds,
            ),
            LocationTrust::Lost
        );
        // 驱动上报 trusted 不能把过期位置升级
        assert_eq!(
            derive_trust(
                PlacementMode::Runtime,
                Some(&timestamp_ago(3600)),
                Some(LocationTrust::Trusted),
                now(),
                thresholds,
            ),
            LocationTrust::Lost
        );
    }

    #[test]
    fn runtime_missing_or_invalid_timestamp_is_lost() {
        let thresholds = TrustThresholds::default();
        assert_eq!(
            derive_trust(
                PlacementMode::Runtime,
                None,
                Some(LocationTrust::Trusted),
                now(),
                thresholds,
            ),
            LocationTrust::Lost
        );
        assert_eq!(
            derive_trust(PlacementMode::Runtime, Some("not-a-timestamp"), None, now(), thresholds,),
            LocationTrust::Lost
        );
    }

    #[test]
    fn runtime_future_timestamp_is_treated_as_fresh() {
        assert_eq!(
            derive_trust(
                PlacementMode::Runtime,
                Some(&timestamp_ago(-60)),
                None,
                now(),
                TrustThresholds::default(),
            ),
            LocationTrust::Trusted
        );
    }

    #[test]
    fn custom_thresholds_are_respected() {
        let thresholds = TrustThresholds::new(chrono::Duration::seconds(5), chrono::Duration::seconds(60));
        assert_eq!(
            derive_trust(PlacementMode::Runtime, Some(&timestamp_ago(5)), None, now(), thresholds,),
            LocationTrust::Trusted
        );
        assert_eq!(
            derive_trust(PlacementMode::Runtime, Some(&timestamp_ago(6)), None, now(), thresholds,),
            LocationTrust::Uncertain
        );
        assert_eq!(
            derive_trust(
                PlacementMode::Runtime,
                Some(&timestamp_ago(61)),
                None,
                now(),
                thresholds,
            ),
            LocationTrust::Lost
        );
    }
}
