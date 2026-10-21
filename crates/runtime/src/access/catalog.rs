//! Sampling catalog: which things the sampler reads, and how demanded
//! properties map back to their manifest source.

use tinyiothub_core::driver::manifest::SourceDecl;

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
}
