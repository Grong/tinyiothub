//! `placement` profile (E4d spec §3): manifest placement refs are structurally
//! sound, and the runtime spatial semantics hold against a synthetic store —
//! map/place structure, point-in-polygon (boundary inclusive, smallest area),
//! explicit `zoneRef` precedence/fail-closed, trust derivation and out-of-bounds
//! behavior.
//!
//! All geometry/trust checks are deterministic and package-independent (zero
//! TTL [`ZoneService`] over an in-memory [`MapStore`]); they only run when the
//! package actually declares map/zone/position placement data. Packages such as
//! the reference runtime driver skip them with explicit evidence while the
//! structural checks still run.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use tinyiothub_core::driver::manifest::{DriverManifest, PlacementDecl, PlacementMode};
use tinyiothub_core::models::thing::Thing;

use crate::acceptance::report::{CheckResult, Level, ProfileReport};
use crate::maps::ZoneService;
use crate::maps::model::{
    Geometry, LocationTrust, MapBounds, MapRecord, PlaceKind, PlaceRecord, ThingPosition, TrustThresholds,
};
use crate::maps::zone::{derive_trust, point_in_polygon, polygon_area};
use crate::ports::{MapStore, ThingCacheSource};
use crate::watch::live_state::LiveState;

const PROFILE: &str = "placement";
const WS: &str = "acceptance-ws";
const MAP_MAIN: &str = "map-main";
const MAP_ANNEX: &str = "map-annex";

/// Map-dependent checks: point selection, refs, trust, bounds and source mode.
const MAP_CHECKS: &[&str] = &[
    "zone.point_in_polygon",
    "zone.smallest_area",
    "zone.explicit_ref",
    "zone.unknown_ref",
    "trust.fixed",
    "trust.thresholds",
    "trust.driver_downgrade",
    "trust.lost",
    "zone.out_of_bounds",
    "zone.source_modes",
];

fn check(name: &str, level: Level, detail: impl Into<String>, evidence: serde_json::Value) -> CheckResult {
    CheckResult {
        profile: PROFILE.into(),
        check: name.into(),
        level,
        detail: detail.into(),
        evidence,
    }
}

fn skip(name: &str, reason: &str, evidence: serde_json::Value) -> CheckResult {
    check(name, Level::Skip, format!("skipped: {reason}"), evidence)
}

pub fn run(dir: &Path) -> ProfileReport {
    let mut checks = Vec::new();
    match crate::access::package::read_package_raw(dir) {
        Ok((_, manifest)) => {
            checks.extend(structural_checks());
            checks.push(refs_check(&manifest));
            if has_map_data(&manifest.placement) {
                let rt = tokio::runtime::Runtime::new().expect("build acceptance runtime");
                checks.extend(rt.block_on(semantic_checks()));
            } else {
                checks.extend(skipped_checks(&manifest.placement));
            }
        }
        Err(e) => checks.push(check(
            "package.read",
            Level::Fail,
            e.to_string(),
            serde_json::json!({"error": e.to_string()}),
        )),
    }
    ProfileReport {
        profile: PROFILE.into(),
        checks,
    }
}

fn has_map_data(placement: &PlacementDecl) -> bool {
    placement.map_ref.is_some() || placement.zone_ref.is_some() || placement.position.is_some()
}

// ── 结构校验（合成包/placement） ──────────────────────────────

fn structural_checks() -> Vec<CheckResult> {
    vec![bounds_check(), geometry_check(), default_unique_check()]
}

/// bounds：全部有限、w/h>0、`x+w`/`y+h` 不溢出。合成地图合法，非法夹具必须被拒。
fn bounds_check() -> CheckResult {
    let maps = synthetic_maps();
    let invalid = [
        MapBounds {
            x: f64::NAN,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        MapBounds {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 10.0,
        },
        MapBounds {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: -1.0,
        },
        MapBounds {
            x: f64::MAX,
            y: 0.0,
            w: f64::MAX,
            h: 10.0,
        },
    ];
    let valid_errors: Vec<String> = maps
        .iter()
        .filter_map(|map| validate_bounds(&map.bounds).err())
        .collect();
    let undetected = invalid.iter().filter(|bounds| validate_bounds(bounds).is_ok()).count();
    let evidence = serde_json::json!({
        "source": "synthetic",
        "maps": maps.iter().map(|map| map.name.clone()).collect::<Vec<_>>(),
        "valid_errors": valid_errors,
        "invalid_rejected": invalid.len() - undetected,
        "invalid_cases": invalid.len(),
    });
    if valid_errors.is_empty() && undetected == 0 {
        check(
            "validator.maps_bounds",
            Level::Pass,
            "synthetic map bounds are finite with positive width/height; invalid bounds rejected",
            evidence,
        )
    } else {
        check(
            "validator.maps_bounds",
            Level::Fail,
            format!("invalid bounds not rejected ({undetected}) or valid maps rejected: {valid_errors:?}"),
            evidence,
        )
    }
}

/// zone：多边形 ≥3 点、坐标有限、无连续重复点、面积 > 0。非法夹具必须被拒。
fn geometry_check() -> CheckResult {
    let places = synthetic_places();
    let valid_errors: Vec<String> = places
        .iter()
        .filter_map(|place| match &place.geometry {
            Geometry::Polygon { points } => validate_zone(points).err(),
            Geometry::Point { .. } => None,
        })
        .collect();
    let invalid: Vec<Vec<(f64, f64)>> = vec![
        vec![(0.0, 0.0), (1.0, 1.0)],
        vec![(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)],
        vec![(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (2.0, 2.0), (0.0, 2.0)],
        vec![(0.0, 0.0), (f64::NAN, 0.0), (2.0, 2.0)],
    ];
    let undetected = invalid.iter().filter(|points| validate_zone(points).is_ok()).count();
    let evidence = serde_json::json!({
        "source": "synthetic",
        "zones": places.iter()
            .filter(|place| place.kind == PlaceKind::Zone)
            .map(|place| place.name.clone())
            .collect::<Vec<_>>(),
        "valid_errors": valid_errors,
        "invalid_rejected": invalid.len() - undetected,
        "invalid_cases": invalid.len(),
    });
    if valid_errors.is_empty() && undetected == 0 {
        check(
            "validator.zone_geometry",
            Level::Pass,
            "synthetic zone polygons have >=3 finite points and positive area; invalid geometry rejected",
            evidence,
        )
    } else {
        check(
            "validator.zone_geometry",
            Level::Fail,
            format!("zone geometry validation incomplete (undetected={undetected}): {valid_errors:?}"),
            evidence,
        )
    }
}

/// 每 workspace 至多一个默认图。合成世界恰好一个；两个默认必须被拒。
fn default_unique_check() -> CheckResult {
    let maps = synthetic_maps();
    let defaults: Vec<String> = maps
        .iter()
        .filter(|map| map.is_default)
        .map(|map| map.name.clone())
        .collect();
    let valid = validate_default_unique(&maps).is_ok() && defaults.len() == 1;
    let mut duplicated = maps.clone();
    if let Some(first) = duplicated.first_mut() {
        first.is_default = true;
    }
    if let Some(second) = duplicated.get_mut(1) {
        second.is_default = true;
    }
    let rejected = validate_default_unique(&duplicated).is_err();
    let evidence = serde_json::json!({
        "source": "synthetic",
        "maps": maps.iter()
            .map(|map| serde_json::json!({"name": map.name, "is_default": map.is_default}))
            .collect::<Vec<_>>(),
        "defaults": defaults,
        "duplicate_rejected": rejected,
    });
    if valid && rejected {
        check(
            "validator.default_unique",
            Level::Pass,
            "synthetic workspace has exactly one default map; duplicate defaults rejected",
            evidence,
        )
    } else {
        check(
            "validator.default_unique",
            Level::Fail,
            format!("default map uniqueness not enforced (valid={valid}, duplicate_rejected={rejected})"),
            evidence,
        )
    }
}

fn validate_bounds(bounds: &MapBounds) -> Result<(), String> {
    if ![bounds.x, bounds.y, bounds.w, bounds.h].iter().all(|v| v.is_finite()) {
        return Err("bounds must be finite".into());
    }
    if bounds.w <= 0.0 || bounds.h <= 0.0 {
        return Err("bounds width and height must be positive".into());
    }
    if !(bounds.x + bounds.w).is_finite() || !(bounds.y + bounds.h).is_finite() {
        return Err("bounds must not overflow".into());
    }
    Ok(())
}

fn validate_zone(points: &[(f64, f64)]) -> Result<(), String> {
    if points.len() < 3 {
        return Err("zone polygon needs at least 3 points".into());
    }
    if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Err("polygon coordinates must be finite".into());
    }
    if points.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("polygon must not contain duplicate consecutive points".into());
    }
    if polygon_area(points) <= 0.0 {
        return Err("zone polygon must have non-zero area".into());
    }
    Ok(())
}

fn validate_default_unique(maps: &[MapRecord]) -> Result<(), String> {
    let defaults = maps.iter().filter(|map| map.is_default).count();
    if defaults > 1 {
        return Err(format!("workspace has {defaults} default maps"));
    }
    Ok(())
}

/// Manifest placement 引用：名字非空、静态 position 结构合法。运行时模式带
/// 静态引用会在绑定时被清空，只告警不失败（spec §2.3）。
fn refs_check(manifest: &DriverManifest) -> CheckResult {
    let placement = &manifest.placement;
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for (label, value) in [("mapRef", &placement.map_ref), ("zoneRef", &placement.zone_ref)] {
        if value.as_deref().is_some_and(|name| name.trim().is_empty()) {
            errors.push(format!("placement.{label} must not be empty"));
        }
    }
    if let Some(raw) = &placement.position {
        match raw.as_object() {
            Some(obj) => {
                for axis in ["x", "y"] {
                    match obj.get(axis).and_then(serde_json::Value::as_f64) {
                        Some(value) if value.is_finite() => {}
                        Some(_) => errors.push(format!("placement.position.{axis} must be finite")),
                        None => errors.push(format!("placement.position.{axis} must be a number")),
                    }
                }
            }
            None => errors.push("placement.position must be an object".into()),
        }
    }
    if placement.mode == PlacementMode::Runtime && has_map_data(placement) {
        warnings.push("runtime placement ignores static mapRef/zoneRef/position (cleared at bind)".to_string());
    }
    // 摄取门要求 `fromSource` 指向已声明的 kind=position source；悬空/错误类型
    // 会让 placement 静默失效，按 fail 处理而不是放过（demand 驱动使用者在验收
    // 阶段就能发现）。
    if let Some(source_id) = placement.from_source.as_deref() {
        match manifest.sources.iter().find(|source| source.id == source_id) {
            None => errors.push(format!("placement.fromSource '{source_id}' is not a declared source")),
            Some(source) if source.kind != "position" => errors.push(format!(
                "placement.fromSource '{source_id}' must declare kind=position (got '{}')",
                source.kind
            )),
            Some(_) => {}
        }
    }

    let evidence = serde_json::json!({
        "mode": match placement.mode {
            PlacementMode::Fixed => "fixed",
            PlacementMode::Declared => "declared",
            PlacementMode::Runtime => "runtime",
        },
        "mapRef": placement.map_ref,
        "zoneRef": placement.zone_ref,
        "position": placement.position,
        "fromSource": placement.from_source,
        "errors": errors,
        "warnings": warnings,
    });
    if !errors.is_empty() {
        check("placement.refs", Level::Fail, errors.join("; "), evidence)
    } else if !warnings.is_empty() {
        check("placement.refs", Level::Warn, warnings.join("; "), evidence)
    } else {
        check(
            "placement.refs",
            Level::Pass,
            "placement refs are well-formed",
            evidence,
        )
    }
}

fn skipped_checks(placement: &PlacementDecl) -> Vec<CheckResult> {
    let reason = "package declares no placement mapRef/zoneRef/position data";
    let evidence = serde_json::json!({
        "reason": reason,
        "mode": match placement.mode {
            PlacementMode::Fixed => "fixed",
            PlacementMode::Declared => "declared",
            PlacementMode::Runtime => "runtime",
        },
        "mapRef": placement.map_ref,
        "zoneRef": placement.zone_ref,
        "position": placement.position,
    });
    MAP_CHECKS
        .iter()
        .map(|name| skip(name, reason, evidence.clone()))
        .collect()
}

// ── 语义校验（合成 store，零 TTL） ───────────────────────────

async fn semantic_checks() -> Vec<CheckResult> {
    vec![
        point_in_polygon_check(),
        smallest_area_check().await,
        explicit_ref_check().await,
        unknown_ref_check().await,
        fixed_trust_check().await,
        thresholds_check(),
        driver_downgrade_check(),
        lost_trust_check().await,
        out_of_bounds_check().await,
        zone_source_check().await,
    ]
}

fn point_in_polygon_check() -> CheckResult {
    let square = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
    let cases: [((f64, f64), bool, &str); 6] = [
        ((1.0, 1.0), true, "interior"),
        ((1.0, 0.0), true, "edge"),
        ((0.0, 0.0), true, "vertex"),
        ((2.0, 1.0), true, "edge-right"),
        ((3.0, 0.0), false, "edge-extension"),
        ((2.5, 1.0), false, "outside"),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter(|(point, expected, _)| point_in_polygon(*point, &square) != *expected)
        .map(|(point, expected, label)| format!("{label} {point:?}: expected {expected}"))
        .collect();
    let evidence = serde_json::json!({
        "cases": cases.iter().map(|(point, expected, label)| serde_json::json!({
            "point": point,
            "inside": point_in_polygon(*point, &square),
            "expected": expected,
            "label": label,
        })).collect::<Vec<_>>(),
    });
    if failures.is_empty() {
        check(
            "zone.point_in_polygon",
            Level::Pass,
            "edges and vertices are inside; extensions/outside are not",
            evidence,
        )
    } else {
        check("zone.point_in_polygon", Level::Fail, failures.join("; "), evidence)
    }
}

async fn smallest_area_check() -> CheckResult {
    let store = Arc::new(
        SyntheticStore::world("maps")
            .with_position("overlap", runtime_position(1.0, 1.0, 0))
            .with_position("outer", runtime_position(5.0, 5.0, 0)),
    );
    let zones = zone_service(store);
    let overlap = zones.zone_for(WS, "overlap", None, now()).await.unwrap();
    let outer = zones.zone_for(WS, "outer", None, now()).await.unwrap();
    let evidence = serde_json::json!({
        "overlap_zone": overlap,
        "expected_overlap": "room",
        "outer_zone": outer,
        "expected_outer": "plant",
    });
    if overlap.as_deref() == Some("room") && outer.as_deref() == Some("plant") {
        check(
            "zone.smallest_area",
            Level::Pass,
            "overlapping zones resolve to the smallest polygon (room over plant/wide)",
            evidence,
        )
    } else {
        check(
            "zone.smallest_area",
            Level::Fail,
            format!("smallest-area selection failed (overlap={overlap:?}, outer={outer:?})"),
            evidence,
        )
    }
}

async fn explicit_ref_check() -> CheckResult {
    let store = Arc::new(
        SyntheticStore::world("maps")
            .with_position("thing", runtime_position(1.0, 1.0, 0))
            .with_zone_ref("thing", "plant"),
    );
    let zones = zone_service(store);
    let stored = zones.zone_for(WS, "thing", None, now()).await.unwrap();
    let parameter = zones.zone_for(WS, "thing", Some("room"), now()).await.unwrap();
    let evidence = serde_json::json!({
        "stored_zone_ref": "plant",
        "point_zone": "room",
        "stored": stored,
        "parameter": parameter,
    });
    if stored.as_deref() == Some("plant") && parameter.as_deref() == Some("room") {
        check(
            "zone.explicit_ref",
            Level::Pass,
            "explicit zoneRef wins over point-in-polygon (stored and call-site)",
            evidence,
        )
    } else {
        check(
            "zone.explicit_ref",
            Level::Fail,
            format!("explicit zoneRef precedence failed (stored={stored:?}, parameter={parameter:?})"),
            evidence,
        )
    }
}

async fn unknown_ref_check() -> CheckResult {
    let store = Arc::new(
        SyntheticStore::world("maps")
            .with_position("thing", runtime_position(1.0, 1.0, 0))
            // `stored` sits inside `room`: a fallback would be observable.
            .with_position("stored", runtime_position(1.0, 1.0, 0))
            .with_zone_ref("stored", "ghost"),
    );
    let zones = zone_service(store);
    let parameter = zones.zone_for(WS, "thing", Some("ghost"), now()).await.unwrap();
    let stored = zones.zone_for(WS, "stored", None, now()).await.unwrap();
    let evidence = serde_json::json!({
        "parameter": parameter,
        "stored": stored,
        "point_zone": "room",
    });
    if parameter.is_none() && stored.is_none() {
        check(
            "zone.unknown_ref",
            Level::Pass,
            "unknown zoneRef fails closed (no point-in-polygon fallback)",
            evidence,
        )
    } else {
        check(
            "zone.unknown_ref",
            Level::Fail,
            format!("unknown zoneRef did not fail closed (parameter={parameter:?}, stored={stored:?})"),
            evidence,
        )
    }
}

async fn fixed_trust_check() -> CheckResult {
    let thresholds = TrustThresholds::default();
    let fixed = derive_trust(
        PlacementMode::Fixed,
        Some(&timestamp_ago(3600)),
        Some(LocationTrust::Lost),
        now(),
        thresholds,
    );
    let declared = derive_trust(
        PlacementMode::Declared,
        None,
        Some(LocationTrust::Lost),
        now(),
        thresholds,
    );
    let store = Arc::new(SyntheticStore::world("maps").with_position("static", static_position(1.0, 1.0)));
    let zones = zone_service(store);
    let zone = zones.zone_for(WS, "static", None, now()).await.unwrap();
    let evidence = serde_json::json!({
        "fixed": fixed,
        "declared": declared,
        "static_zone": zone,
        "driver_reported": "lost",
    });
    if fixed == LocationTrust::Trusted && declared == LocationTrust::Trusted && zone.as_deref() == Some("room") {
        check(
            "trust.fixed",
            Level::Pass,
            "fixed/declared static positions stay trusted regardless of age or driver trust",
            evidence,
        )
    } else {
        check("trust.fixed", Level::Fail, "static placement was not trusted", evidence)
    }
}

fn thresholds_check() -> CheckResult {
    let thresholds = TrustThresholds::default();
    let trusted_max = thresholds.trusted_max.num_seconds();
    let uncertain_max = thresholds.uncertain_max.num_seconds();
    // Probe both inclusive boundaries and the first second past them.
    let cases = [
        (0, LocationTrust::Trusted),
        (trusted_max, LocationTrust::Trusted),
        (trusted_max + 1, LocationTrust::Uncertain),
        (uncertain_max, LocationTrust::Uncertain),
        (uncertain_max + 1, LocationTrust::Lost),
    ];
    let results: Vec<(i64, LocationTrust, LocationTrust)> = cases
        .iter()
        .map(|(age, expected)| {
            (
                *age,
                derive_trust(
                    PlacementMode::Runtime,
                    Some(&timestamp_ago(*age)),
                    None,
                    now(),
                    thresholds,
                ),
                *expected,
            )
        })
        .collect();
    let failures: Vec<String> = results
        .iter()
        .filter(|(_, got, expected)| got != expected)
        .map(|(age, got, expected)| format!("age={age}s got {got:?}, expected {expected:?}"))
        .collect();
    let evidence = serde_json::json!({
        "cases": results.iter().map(|(age, got, expected)| serde_json::json!({
            "age_secs": age,
            "got": got,
            "expected": expected,
        })).collect::<Vec<_>>(),
        "trusted_max_secs": trusted_max,
        "uncertain_max_secs": uncertain_max,
    });
    if failures.is_empty() {
        check(
            "trust.thresholds",
            Level::Pass,
            format!("runtime trust: <={trusted_max}s trusted, <={uncertain_max}s uncertain, >{uncertain_max}s lost"),
            evidence,
        )
    } else {
        check("trust.thresholds", Level::Fail, failures.join("; "), evidence)
    }
}

fn driver_downgrade_check() -> CheckResult {
    let thresholds = TrustThresholds::default();
    let uncertain = derive_trust(
        PlacementMode::Runtime,
        Some(&timestamp_ago(0)),
        Some(LocationTrust::Uncertain),
        now(),
        thresholds,
    );
    let lost = derive_trust(
        PlacementMode::Runtime,
        Some(&timestamp_ago(0)),
        Some(LocationTrust::Lost),
        now(),
        thresholds,
    );
    let stale_trusted = derive_trust(
        PlacementMode::Runtime,
        Some(&timestamp_ago(3600)),
        Some(LocationTrust::Trusted),
        now(),
        thresholds,
    );
    let evidence = serde_json::json!({
        "fresh_driver_uncertain": uncertain,
        "fresh_driver_lost": lost,
        "stale_driver_trusted": stale_trusted,
    });
    if uncertain == LocationTrust::Uncertain && lost == LocationTrust::Lost && stale_trusted == LocationTrust::Lost {
        check(
            "trust.driver_downgrade",
            Level::Pass,
            "driver trust can only downgrade the age-derived trust",
            evidence,
        )
    } else {
        check(
            "trust.driver_downgrade",
            Level::Fail,
            "driver trust upgraded or failed to downgrade age-derived trust",
            evidence,
        )
    }
}

async fn lost_trust_check() -> CheckResult {
    let mut position = runtime_position(1.0, 1.0, 0);
    position.trust = LocationTrust::Lost;
    let store = Arc::new(SyntheticStore::world("maps").with_position("lost", position));
    let zones = zone_service(store);
    let zone = zones.zone_for(WS, "lost", None, now()).await.unwrap();
    let evidence = serde_json::json!({"zone": zone, "trust": "lost"});
    if zone.is_none() {
        check("trust.lost", Level::Pass, "Lost trust yields zone None", evidence)
    } else {
        check(
            "trust.lost",
            Level::Fail,
            format!("Lost trust still yielded {zone:?}"),
            evidence,
        )
    }
}

async fn out_of_bounds_check() -> CheckResult {
    let position = runtime_position(150.0, 50.0, 0);
    let store = Arc::new(SyntheticStore::world("maps").with_position("oob", position.clone()));
    let zones = zone_service(store.clone());
    let zone = zones.zone_for(WS, "oob", None, now()).await.unwrap();
    let out_of_bounds = zones.position_out_of_bounds(WS, &position).await.unwrap();
    let retained = store.position("oob");
    let retained_ok = retained
        .as_ref()
        .is_some_and(|stored| stored.x == Some(150.0) && stored.y == Some(50.0));
    let evidence = serde_json::json!({
        "zone": zone,
        "out_of_bounds": out_of_bounds,
        "position": retained,
        "coordinates_retained": retained_ok,
    });
    if zone.is_none() && out_of_bounds == Some(true) && retained_ok {
        check(
            "zone.out_of_bounds",
            Level::Pass,
            "out-of-bounds position keeps coordinates but yields zone None",
            evidence,
        )
    } else {
        check(
            "zone.out_of_bounds",
            Level::Fail,
            format!(
                "out-of-bounds behavior wrong (zone={zone:?}, out_of_bounds={out_of_bounds:?}, retained={retained_ok})"
            ),
            evidence,
        )
    }
}

/// Exercises the real authority gate in [`LiveState::derive_zone`]: maps data is
/// present the whole time, so only `zone_source` decides between polygons and
/// the hierarchy ancestor chain.
async fn zone_source_check() -> CheckResult {
    // Static position (no updated_at): LiveState uses the wall clock for trust,
    // so a fixed timestamp would age out of the test window.
    let store = Arc::new(SyntheticStore::world("hierarchy").with_position("gate-thing", static_position(1.0, 1.0)));
    let mut live =
        LiveState::with_zone_service(Arc::new(GateCache::hierarchy()), Arc::new(zone_service(store.clone())));
    live.handle_lagged();

    let hierarchy = live.summary("gate-thing").await.expect("gate thing is cached").zone;
    store.set_source("maps");
    let maps = live.summary("gate-thing").await.expect("gate thing is cached").zone;
    store.set_source("hierarchy");
    let hierarchy_restored = live.summary("gate-thing").await.expect("gate thing is cached").zone;

    let evidence = serde_json::json!({
        "gate": "LiveState::derive_zone",
        "maps_data_present_throughout": true,
        "hierarchy": hierarchy,
        "maps": maps,
        "hierarchy_restored": hierarchy_restored,
        "expected_hierarchy": "hierarchy-site",
        "expected_maps": "room",
    });
    if hierarchy == "hierarchy-site" && maps == "room" && hierarchy_restored == "hierarchy-site" {
        check(
            "zone.source_modes",
            Level::Pass,
            "LiveState gate: hierarchy source uses ancestors even with maps data; maps source uses polygons; switching back restores hierarchy",
            evidence,
        )
    } else {
        check(
            "zone.source_modes",
            Level::Fail,
            format!(
                "LiveState zone-authority gate wrong (hierarchy={hierarchy:?}, maps={maps:?}, restored={hierarchy_restored:?})"
            ),
            evidence,
        )
    }
}

/// Minimal cache for the gate check: child `gate-thing` under `gate-parent`,
/// both in [`WS`]. Hierarchy zone = parent display name.
struct GateCache(Mutex<HashMap<String, Thing>>);

impl GateCache {
    fn hierarchy() -> Self {
        let parent = Thing {
            id: "gate-parent".into(),
            name: "gate-parent".into(),
            display_name: Some("hierarchy-site".into()),
            workspace_id: Some(WS.into()),
            ..Thing::default()
        };
        let child = Thing {
            id: "gate-thing".into(),
            name: "gate-thing".into(),
            display_name: Some("gate-thing".into()),
            workspace_id: Some(WS.into()),
            parent_id: Some("gate-parent".into()),
            ..Thing::default()
        };
        Self(Mutex::new(HashMap::from([
            (parent.id.clone(), parent),
            (child.id.clone(), child),
        ])))
    }
}

impl ThingCacheSource for GateCache {
    fn all(&self) -> Vec<Thing> {
        self.0.lock().unwrap().values().cloned().collect()
    }

    fn get(&self, id: &str) -> Option<Thing> {
        self.0.lock().unwrap().get(id).cloned()
    }

    fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.0
            .lock()
            .unwrap()
            .values()
            .find(|thing| thing.name == name)
            .cloned()
    }

    fn insert(&self, device: Thing) {
        self.0.lock().unwrap().insert(device.id.clone(), device);
    }

    fn update(&self, device: Thing) {
        self.0.lock().unwrap().insert(device.id.clone(), device);
    }

    fn remove(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
}

// ── 合成地图/地点/坐标 ───────────────────────────────────────

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap()
}

fn timestamp_ago(seconds: i64) -> String {
    (now() - chrono::Duration::seconds(seconds)).to_rfc3339()
}

fn runtime_position(x: f64, y: f64, age_secs: i64) -> ThingPosition {
    ThingPosition {
        map_id: None,
        x: Some(x),
        y: Some(y),
        level: None,
        heading: None,
        trust: LocationTrust::Trusted,
        updated_at: Some(timestamp_ago(age_secs)),
    }
}

fn static_position(x: f64, y: f64) -> ThingPosition {
    ThingPosition {
        map_id: None,
        x: Some(x),
        y: Some(y),
        level: None,
        heading: None,
        trust: LocationTrust::Lost,
        updated_at: None,
    }
}

fn map_rec(id: &str, name: &str, bounds: MapBounds, is_default: bool) -> MapRecord {
    MapRecord {
        id: id.into(),
        workspace_id: WS.into(),
        name: name.into(),
        bounds,
        anchor_map_id: None,
        is_default,
        created_at: "2026-10-22T00:00:00Z".into(),
        updated_at: "2026-10-22T00:00:00Z".into(),
    }
}

fn zone_rec(id: &str, map_id: &str, name: &str, points: &[(f64, f64)]) -> PlaceRecord {
    PlaceRecord {
        id: id.into(),
        map_id: map_id.into(),
        name: name.into(),
        kind: PlaceKind::Zone,
        geometry: Geometry::Polygon {
            points: points.to_vec(),
        },
        heading: None,
        created_at: "2026-10-22T00:00:00Z".into(),
        updated_at: "2026-10-22T00:00:00Z".into(),
    }
}

fn synthetic_maps() -> Vec<MapRecord> {
    vec![
        map_rec(
            MAP_MAIN,
            "main",
            MapBounds {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            true,
        ),
        map_rec(
            MAP_ANNEX,
            "annex",
            MapBounds {
                x: 200.0,
                y: 0.0,
                w: 50.0,
                h: 50.0,
            },
            false,
        ),
    ]
}

/// `room` (area 4) sits inside `plant` (100) which sits inside `wide` (20000):
/// overlapping hits resolve by smallest area.
fn synthetic_places() -> Vec<PlaceRecord> {
    vec![
        zone_rec(
            "z-wide",
            MAP_MAIN,
            "wide",
            &[(0.0, 0.0), (200.0, 0.0), (200.0, 100.0), (0.0, 100.0)],
        ),
        zone_rec(
            "z-plant",
            MAP_MAIN,
            "plant",
            &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        ),
        zone_rec(
            "z-room",
            MAP_MAIN,
            "room",
            &[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)],
        ),
        zone_rec(
            "z-annex",
            MAP_ANNEX,
            "annex-zone",
            &[(200.0, 0.0), (210.0, 0.0), (210.0, 10.0), (200.0, 10.0)],
        ),
    ]
}

fn zone_service(store: Arc<SyntheticStore>) -> ZoneService {
    ZoneService::with_ttl(store, TrustThresholds::default(), Duration::ZERO)
}

/// 内存 MapStore：maps/places/position/zone_ref/ingest 全部可变，零外部依赖。
struct SyntheticStore {
    source: Mutex<String>,
    maps: Vec<MapRecord>,
    places: Vec<PlaceRecord>,
    positions: Mutex<HashMap<String, ThingPosition>>,
    zone_refs: Mutex<HashMap<String, String>>,
    ingest: Mutex<HashMap<String, bool>>,
}

impl SyntheticStore {
    fn world(source: &str) -> Self {
        Self::new(source, synthetic_maps(), synthetic_places())
    }

    fn new(source: &str, maps: Vec<MapRecord>, places: Vec<PlaceRecord>) -> Self {
        Self {
            source: Mutex::new(source.into()),
            maps,
            places,
            positions: Mutex::new(HashMap::new()),
            zone_refs: Mutex::new(HashMap::new()),
            ingest: Mutex::new(HashMap::new()),
        }
    }

    fn with_position(self, thing_id: &str, position: ThingPosition) -> Self {
        self.positions.lock().unwrap().insert(thing_id.into(), position);
        self
    }

    fn with_zone_ref(self, thing_id: &str, zone_ref: &str) -> Self {
        self.zone_refs.lock().unwrap().insert(thing_id.into(), zone_ref.into());
        self
    }

    fn position(&self, thing_id: &str) -> Option<ThingPosition> {
        self.positions.lock().unwrap().get(thing_id).cloned()
    }

    fn set_source(&self, source: &str) {
        *self.source.lock().unwrap() = source.into();
    }
}

#[async_trait]
impl MapStore for SyntheticStore {
    async fn list_maps(&self, _workspace_id: &str) -> Result<Vec<MapRecord>, String> {
        Ok(self.maps.clone())
    }

    async fn default_map(&self, _workspace_id: &str) -> Result<Option<MapRecord>, String> {
        Ok(self.maps.iter().find(|map| map.is_default).cloned())
    }

    async fn upsert_map(&self, _map: &MapRecord) -> Result<(), String> {
        Ok(())
    }

    async fn delete_map(&self, _workspace_id: &str, _id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn list_places(&self, _workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, String> {
        Ok(self
            .places
            .iter()
            .filter(|place| place.map_id == map_id)
            .cloned()
            .collect())
    }

    async fn find_place_by_name(
        &self,
        _workspace_id: &str,
        name: &str,
        kind: Option<PlaceKind>,
    ) -> Result<Option<PlaceRecord>, String> {
        Ok(self
            .places
            .iter()
            .find(|place| place.name == name && kind.is_none_or(|kind| place.kind == kind))
            .cloned())
    }

    async fn upsert_place(&self, _workspace_id: &str, _place: &PlaceRecord) -> Result<(), String> {
        Ok(())
    }

    async fn delete_place(&self, _workspace_id: &str, _map_id: &str, _id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn get_position(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<ThingPosition>, String> {
        Ok(self.position(thing_id))
    }

    async fn set_position(&self, _workspace_id: &str, thing_id: &str, position: &ThingPosition) -> Result<(), String> {
        self.positions.lock().unwrap().insert(thing_id.into(), position.clone());
        Ok(())
    }

    async fn set_reading_position(
        &self,
        _workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<(), String> {
        self.positions.lock().unwrap().insert(thing_id.into(), position.clone());
        Ok(())
    }

    async fn list_positions(&self, _workspace_id: &str) -> Result<Vec<(String, ThingPosition)>, String> {
        Ok(self
            .positions
            .lock()
            .unwrap()
            .iter()
            .map(|(thing_id, position)| (thing_id.clone(), position.clone()))
            .collect())
    }

    async fn get_zone_ref(&self, _workspace_id: &str, thing_id: &str) -> Result<Option<String>, String> {
        Ok(self.zone_refs.lock().unwrap().get(thing_id).cloned())
    }

    async fn set_zone_ref(&self, _workspace_id: &str, thing_id: &str, zone_ref: Option<&str>) -> Result<(), String> {
        let mut refs = self.zone_refs.lock().unwrap();
        match zone_ref {
            Some(name) => {
                refs.insert(thing_id.into(), name.into());
            }
            None => {
                refs.remove(thing_id);
            }
        }
        Ok(())
    }

    async fn list_zone_refs(&self, _workspace_id: &str) -> Result<Vec<(String, String)>, String> {
        Ok(self
            .zone_refs
            .lock()
            .unwrap()
            .iter()
            .map(|(thing_id, zone_ref)| (thing_id.clone(), zone_ref.clone()))
            .collect())
    }

    async fn get_position_ingest(&self, _workspace_id: &str, thing_id: &str) -> Result<bool, String> {
        Ok(self.ingest.lock().unwrap().get(thing_id).copied().unwrap_or(false))
    }

    async fn set_position_ingest(&self, _workspace_id: &str, thing_id: &str, ingest: bool) -> Result<(), String> {
        self.ingest.lock().unwrap().insert(thing_id.into(), ingest);
        Ok(())
    }

    async fn zone_source(&self, _workspace_id: &str) -> Result<String, String> {
        Ok(self.source.lock().unwrap().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUNTIME_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"demo","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],"stateFields":[],
      "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
      "resources":[],"placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    const STATIC_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"spatial","version":"1.0.0","protocol":"demo","abiVersion":2,"dynamicCapabilities":false},
      "config":{"type":"object"},"tools":[],"stateFields":[],
      "sources":[{"id":"telemetry","kind":"poll","fields":[]}],
      "resources":[],
      "placement":{"mode":"fixed","mapRef":"main","zoneRef":"room","position":{"x":1,"y":1},"trust":"trusted"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    fn package(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("driver.manifest.json"), manifest).unwrap();
        dir
    }

    fn level_of(report: &ProfileReport, name: &str) -> Option<Level> {
        report.checks.iter().find(|c| c.check == name).map(|c| c.level)
    }

    #[test]
    fn placement_data_runs_every_check_and_passes() {
        let dir = package(STATIC_MANIFEST);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        for name in [
            "validator.maps_bounds",
            "validator.zone_geometry",
            "validator.default_unique",
            "placement.refs",
            "zone.point_in_polygon",
            "zone.smallest_area",
            "zone.explicit_ref",
            "zone.unknown_ref",
            "trust.fixed",
            "trust.thresholds",
            "trust.driver_downgrade",
            "trust.lost",
            "zone.out_of_bounds",
            "zone.source_modes",
        ] {
            assert_eq!(level_of(&report, name), Some(Level::Pass), "{name}: {report:?}");
        }
        assert_eq!(report.checks.len(), 4 + MAP_CHECKS.len());
    }

    #[test]
    fn package_without_placement_data_skips_map_checks_with_evidence() {
        let dir = package(RUNTIME_MANIFEST);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Pass, "{report:?}");
        assert_eq!(level_of(&report, "validator.maps_bounds"), Some(Level::Pass));
        assert_eq!(level_of(&report, "validator.zone_geometry"), Some(Level::Pass));
        assert_eq!(level_of(&report, "validator.default_unique"), Some(Level::Pass));
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Pass));
        for name in MAP_CHECKS {
            let check = report.checks.iter().find(|c| c.check == *name).unwrap();
            assert_eq!(check.level, Level::Skip, "{name}: {report:?}");
            assert!(check.detail.contains("skipped"), "{check:?}");
            assert_eq!(
                check.evidence["reason"],
                "package declares no placement mapRef/zoneRef/position data"
            );
        }
    }

    #[test]
    fn runtime_static_refs_warn_but_do_not_fail() {
        let manifest = STATIC_MANIFEST.replace("\"mode\":\"fixed\"", "\"mode\":\"runtime\"");
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Warn, "{report:?}");
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Warn));
        // Map-dependent checks still run: the synthetic world is independent of mode.
        assert_eq!(level_of(&report, "zone.smallest_area"), Some(Level::Pass));
    }

    #[test]
    fn empty_zone_ref_fails_refs_check() {
        let manifest = STATIC_MANIFEST.replace("\"zoneRef\":\"room\"", "\"zoneRef\":\" \"");
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail, "{report:?}");
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Fail));
    }

    #[test]
    fn dangling_from_source_fails_refs_check() {
        let manifest = RUNTIME_MANIFEST.replace(
            "\"placement\":{\"mode\":\"runtime\"}",
            "\"placement\":{\"mode\":\"runtime\",\"fromSource\":\"position\"}",
        );
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail, "{report:?}");
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Fail));
        let refs = report.checks.iter().find(|c| c.check == "placement.refs").unwrap();
        assert!(refs.detail.contains("not a declared source"), "{refs:?}");
    }

    #[test]
    fn from_source_with_wrong_kind_fails_refs_check() {
        let manifest = RUNTIME_MANIFEST.replace(
            "\"placement\":{\"mode\":\"runtime\"}",
            "\"placement\":{\"mode\":\"runtime\",\"fromSource\":\"telemetry\"}",
        );
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail, "{report:?}");
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Fail));
        let refs = report.checks.iter().find(|c| c.check == "placement.refs").unwrap();
        assert!(refs.detail.contains("kind=position"), "{refs:?}");
    }

    #[test]
    fn declared_position_source_passes_refs_check() {
        let manifest = RUNTIME_MANIFEST
            .replace(
                r#""sources":[{"id":"telemetry","kind":"poll","fields":[]}]"#,
                r#""sources":[{"id":"position","kind":"position","fields":["position_x","position_y"]}]"#,
            )
            .replace(
                "\"placement\":{\"mode\":\"runtime\"}",
                "\"placement\":{\"mode\":\"runtime\",\"fromSource\":\"position\"}",
            );
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Pass), "{report:?}");
        assert_eq!(report.level(), Level::Pass, "{report:?}");
    }

    #[test]
    fn non_finite_position_fails_refs_check() {
        let manifest = STATIC_MANIFEST.replace(r#""position":{"x":1,"y":1}"#, r#""position":{"x":"nope","y":1}"#);
        let dir = package(&manifest);
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail, "{report:?}");
        assert_eq!(level_of(&report, "placement.refs"), Some(Level::Fail));
    }

    #[test]
    fn unreadable_package_fails_package_read() {
        let dir = tempfile::tempdir().unwrap();
        let report = run(dir.path());
        assert_eq!(report.level(), Level::Fail);
        assert_eq!(level_of(&report, "package.read"), Some(Level::Fail));
    }

    #[test]
    fn synthetic_geometry_matches_zone_functions() {
        for map in synthetic_maps() {
            assert!(validate_bounds(&map.bounds).is_ok());
        }
        for place in synthetic_places() {
            if let Geometry::Polygon { points } = &place.geometry {
                assert!(validate_zone(points).is_ok(), "{}", place.name);
            }
        }
        assert_eq!(synthetic_maps().iter().filter(|map| map.is_default).count(), 1);
    }
}
