//! Sampling catalog: which things the sampler reads, and how demanded
//! properties map back to their manifest source.

use std::collections::HashSet;
use std::sync::Arc;

use tinyiothub_core::driver::manifest::SourceDecl;

use crate::access::registry::ManifestRegistry;
use crate::ports::ThingCacheSource;

/// The manifest source kind that can be sampled on a schedule.
const POLL_KIND: &str = "poll";

/// One thing in the sampling catalog, with the sources declared by its
/// driver manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct SampledThing {
    pub workspace_id: String,
    pub thing_id: String,
    pub driver_name: String,
    pub sources: Vec<SourceDecl>,
}

/// Source of the sampler's work list (device cache + manifests in cloud).
pub trait SamplingCatalog: Send + Sync {
    fn list(&self) -> Vec<SampledThing>;
}

/// Production catalog: every cached thing with a driver + that driver's
/// installed manifest sources.
///
/// Things without a driver or whose manifest cannot be found are skipped
/// (fail-closed, spec §3.1 `sampling_no_manifest`).
pub struct CachedManifestCatalog {
    device_cache: Arc<dyn ThingCacheSource>,
    manifests: Arc<ManifestRegistry>,
    /// 上一轮缺失 manifest 的 thing id；只在进入/离开缺失状态时记日志
    /// （catalog 每 50ms 列一次，逐 tick 记会刷屏）。
    missing_manifests: parking_lot::Mutex<HashSet<String>>,
}

impl CachedManifestCatalog {
    pub fn new(device_cache: Arc<dyn ThingCacheSource>, manifests: Arc<ManifestRegistry>) -> Self {
        Self {
            device_cache,
            manifests,
            missing_manifests: parking_lot::Mutex::new(HashSet::new()),
        }
    }
}

impl SamplingCatalog for CachedManifestCatalog {
    fn list(&self) -> Vec<SampledThing> {
        let mut sampled = Vec::new();
        let mut missing = HashSet::new();
        for device in self.device_cache.all() {
            let Some(driver_name) = device.driver_name.clone() else {
                continue;
            };
            let workspace_id = device.workspace_id.clone().unwrap_or_default();
            let Some(package) = self.manifests.get(&workspace_id, &driver_name) else {
                missing.insert(device.id);
                continue;
            };
            sampled.push(SampledThing {
                workspace_id,
                thing_id: device.id,
                driver_name,
                sources: package.manifest.sources.clone(),
            });
        }

        {
            let mut previously_missing = self.missing_manifests.lock();
            for thing_id in missing.difference(&previously_missing) {
                tracing::debug!(
                    thing_id = %thing_id,
                    "sampling_no_manifest：manifest 缺失，跳过采样（fail-closed）"
                );
            }
            for thing_id in previously_missing.difference(&missing) {
                tracing::debug!(
                    thing_id = %thing_id,
                    "sampling_manifest_recovered：manifest 已可用，恢复采样"
                );
            }
            *previously_missing = missing;
        }
        sampled
    }
}

fn is_poll(source: &SourceDecl) -> bool {
    source.kind == POLL_KIND
}

/// Map a demanded property to a source id.
///
/// A `poll` source that declares the property in `fields` wins. Otherwise, if
/// exactly one `poll` source exists (dynamic drivers may leave `fields` empty),
/// that source is the fallback regardless of its `health` flag. Non-poll
/// sources are never candidates. An empty property (thing-level watch) maps to
/// nothing.
pub fn source_for_property(sources: &[SourceDecl], property: &str) -> Option<String> {
    if property.is_empty() {
        return None;
    }
    if let Some(matched) = sources
        .iter()
        .filter(|source| is_poll(source))
        .find(|source| source.fields.iter().any(|field| field == property))
    {
        return Some(matched.id.clone());
    }
    let mut candidates = sources.iter().filter(|source| is_poll(source));
    match (candidates.next(), candidates.next()) {
        (Some(source), None) => Some(source.id.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(id: &str, kind: &str, fields: &[&str]) -> SourceDecl {
        SourceDecl {
            id: id.into(),
            kind: kind.into(),
            fields: fields.iter().map(|field| (*field).into()).collect(),
            interval_ms: 1000,
            min_interval_ms: 200,
            demand_only: false,
            health: false,
        }
    }

    fn health_src(id: &str, fields: &[&str]) -> SourceDecl {
        SourceDecl {
            health: true,
            ..src(id, "poll", fields)
        }
    }

    #[test]
    fn exact_field_match_wins_over_fallback() {
        let sources = vec![
            src("telemetry", "poll", &["level", "temperature"]),
            src("secondary", "poll", &["temperature"]),
        ];
        assert_eq!(source_for_property(&sources, "level"), Some("telemetry".into()));
        assert_eq!(source_for_property(&sources, "temperature"), Some("telemetry".into()));
    }

    #[test]
    fn exact_field_match_scans_all_poll_sources() {
        let sources = vec![src("telemetry", "poll", &["level"]), src("energy", "poll", &["kwh"])];
        assert_eq!(source_for_property(&sources, "kwh"), Some("energy".into()));
    }

    #[test]
    fn single_dynamic_poll_source_is_fallback() {
        let sources = vec![src("telemetry", "poll", &[])];
        assert_eq!(source_for_property(&sources, "anything"), Some("telemetry".into()));
    }

    #[test]
    fn fallback_requires_exactly_one_poll_source() {
        let sources = vec![src("telemetry", "poll", &[]), src("energy", "poll", &[])];
        assert_eq!(source_for_property(&sources, "level"), None);
    }

    #[test]
    fn sole_health_poll_source_is_the_fallback() {
        let sources = vec![health_src("health", &[])];
        assert_eq!(source_for_property(&sources, "level"), Some("health".into()));
    }

    #[test]
    fn sole_poll_source_with_other_fields_is_the_fallback() {
        let sources = vec![src("telemetry", "poll", &["level"])];
        assert_eq!(source_for_property(&sources, "temperature"), Some("telemetry".into()));
    }

    #[test]
    fn two_poll_sources_one_health_without_exact_match_is_ambiguous() {
        let sources = vec![src("telemetry", "poll", &[]), health_src("health", &["online"])];
        assert_eq!(source_for_property(&sources, "level"), None);
    }

    #[test]
    fn health_poll_source_can_match_exact_field() {
        let sources = vec![src("telemetry", "poll", &["level"]), health_src("health", &["online"])];
        assert_eq!(source_for_property(&sources, "online"), Some("health".into()));
    }

    #[test]
    fn non_poll_sources_are_not_candidates() {
        let sources = vec![src("events", "push", &["level"]), src("track", "position", &["lat"])];
        assert_eq!(source_for_property(&sources, "level"), None);
        assert_eq!(source_for_property(&sources, "lat"), None);
    }

    #[test]
    fn empty_property_maps_to_nothing() {
        let sources = vec![src("telemetry", "poll", &[])];
        assert_eq!(source_for_property(&sources, ""), None);
    }

    #[test]
    fn catalog_lists_sampled_things_through_dyn_trait() {
        let catalog = FakeCatalog {
            things: vec![SampledThing {
                workspace_id: "ws".into(),
                thing_id: "t1".into(),
                driver_name: "simulator".into(),
                sources: vec![src("telemetry", "poll", &["level"])],
            }],
        };
        let dyn_catalog: &dyn SamplingCatalog = &catalog;
        let listed = dyn_catalog.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].workspace_id, "ws");
        assert_eq!(listed[0].thing_id, "t1");
        assert_eq!(listed[0].driver_name, "simulator");
        assert_eq!(listed[0].sources[0].id, "telemetry");
    }

    struct FakeCatalog {
        things: Vec<SampledThing>,
    }

    impl SamplingCatalog for FakeCatalog {
        fn list(&self) -> Vec<SampledThing> {
            self.things.clone()
        }
    }

    struct FakeCache(std::sync::Mutex<Vec<tinyiothub_core::models::thing::Thing>>);

    impl ThingCacheSource for FakeCache {
        fn all(&self) -> Vec<tinyiothub_core::models::thing::Thing> {
            self.0.lock().unwrap().clone()
        }
        fn get(&self, id: &str) -> Option<tinyiothub_core::models::thing::Thing> {
            self.0.lock().unwrap().iter().find(|thing| thing.id == id).cloned()
        }
        fn get_by_name(&self, name: &str) -> Option<tinyiothub_core::models::thing::Thing> {
            self.0.lock().unwrap().iter().find(|thing| thing.name == name).cloned()
        }
        fn insert(&self, _thing: tinyiothub_core::models::thing::Thing) {}
        fn update(&self, _thing: tinyiothub_core::models::thing::Thing) {}
        fn remove(&self, _id: &str) {}
    }

    fn cached_thing(id: &str, driver: Option<&str>) -> tinyiothub_core::models::thing::Thing {
        tinyiothub_core::models::thing::Thing {
            id: id.into(),
            name: id.into(),
            workspace_id: Some("ws".into()),
            driver_name: driver.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn cached_catalog_skips_missing_manifests_and_recovers() {
        let manifests = Arc::new(ManifestRegistry::new());
        let simulator = include_str!("../../manifests/simulator.json");
        manifests.seed_builtin(simulator).unwrap();

        let cache = Arc::new(FakeCache(std::sync::Mutex::new(vec![
            cached_thing("good", Some("simulator")),
            cached_thing("unknown", Some("nope")),
            cached_thing("no-driver", None),
        ])));
        let catalog = CachedManifestCatalog::new(cache, manifests.clone());

        let listed = catalog.list();
        assert_eq!(listed.len(), 1, "无 manifest / 无 driver 的物必须跳过");
        assert_eq!(listed[0].thing_id, "good");
        assert!(!listed[0].sources.is_empty());

        // 连续列 catalog 不重复记录缺失（行为稳定）。
        assert_eq!(catalog.list().len(), 1);

        // 安装缺失驱动的 manifest 后，下一轮自动恢复。
        let mut renamed: serde_json::Value = serde_json::from_str(simulator).unwrap();
        renamed["identity"]["driver"] = serde_json::Value::String("nope".into());
        manifests.seed_builtin(&renamed.to_string()).unwrap();
        let listed = catalog.list();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|thing| thing.thing_id == "unknown"));
    }
}
