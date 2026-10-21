//! Demand tracking: port snapshot ∪ explicit subscriptions ∪ recent reads.
//!
//! Demand is the union of three sources:
//! - `snapshot`: the [`DemandSource`](crate::ports::DemandSource) port's
//!   property references, replaced wholesale on every poll (no TTL);
//! - explicit subscriptions (Hub stream connections), keyed by workspace,
//!   thing and source, refreshed by `touch` and expiring after their TTL;
//! - recent reads, expiring [`RECENT_READ_TTL`] after the last read.
//!
//! All keys are multi-tenant: `(workspace_id, thing_id)` /
//! `(workspace_id, thing_id, property)`.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::ports::DemandPropertyRef;

/// How long a read keeps its property in demand.
pub const RECENT_READ_TTL: Duration = Duration::from_secs(60);
/// Default lifetime of an explicit subscription; `touch` renews it.
pub const SUBSCRIPTION_TTL: Duration = Duration::from_secs(30);

pub type SubscriptionId = u64;

#[derive(Debug, Clone)]
struct Subscription {
    workspace_id: String,
    things: Vec<String>,
    sources: Vec<String>,
    ttl: Duration,
    expires_at: Instant,
}

pub struct DemandTracker {
    /// `(workspace, thing) -> properties`; empty property = thing-level watch.
    snapshot: RwLock<HashMap<(String, String), BTreeSet<String>>>,
    subscriptions: RwLock<HashMap<SubscriptionId, Subscription>>,
    /// `(workspace, thing, property) -> last read`.
    recent_reads: RwLock<HashMap<(String, String, String), Instant>>,
    next_id: AtomicU64,
}

impl DemandTracker {
    pub fn new() -> Self {
        Self {
            snapshot: RwLock::new(HashMap::new()),
            subscriptions: RwLock::new(HashMap::new()),
            recent_reads: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
        }
    }

    /// Record a read at the current instant.
    pub fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
        self.record_read_at(workspace_id, thing_id, property, Instant::now());
    }

    /// Record a read at `now` (test/simulation entry point).
    pub fn record_read_at(&self, workspace_id: &str, thing_id: &str, property: &str, now: Instant) {
        self.recent_reads.write().insert(
            (workspace_id.to_string(), thing_id.to_string(), property.to_string()),
            now,
        );
    }

    /// Register an explicit subscription at the current instant.
    pub fn subscribe(
        &self,
        workspace_id: &str,
        things: &[String],
        sources: &[String],
        ttl: Duration,
    ) -> SubscriptionId {
        self.subscribe_at(workspace_id, things, sources, ttl, Instant::now())
    }

    /// Register a subscription at `now` (test/simulation entry point).
    pub fn subscribe_at(
        &self,
        workspace_id: &str,
        things: &[String],
        sources: &[String],
        ttl: Duration,
        now: Instant,
    ) -> SubscriptionId {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.subscriptions.write().insert(
            id,
            Subscription {
                workspace_id: workspace_id.to_string(),
                things: things.to_vec(),
                sources: sources.to_vec(),
                ttl,
                expires_at: now + ttl,
            },
        );
        id
    }

    /// Renew a subscription for another full TTL. Returns false if unknown.
    pub fn touch(&self, id: &SubscriptionId) -> bool {
        self.touch_at(id, Instant::now())
    }

    /// Renew at `now` (test/simulation entry point).
    pub fn touch_at(&self, id: &SubscriptionId, now: Instant) -> bool {
        let mut subscriptions = self.subscriptions.write();
        match subscriptions.get_mut(id) {
            Some(subscription) => {
                subscription.expires_at = now + subscription.ttl;
                true
            }
            None => false,
        }
    }

    pub fn unsubscribe(&self, id: &SubscriptionId) {
        self.subscriptions.write().remove(id);
    }

    /// Replace the whole port snapshot.
    pub fn set_snapshot(&self, refs: Vec<DemandPropertyRef>) {
        let mut snapshot: HashMap<(String, String), BTreeSet<String>> = HashMap::new();
        for reference in refs {
            snapshot
                .entry((reference.workspace_id, reference.thing_id))
                .or_default()
                .insert(reference.property);
        }
        *self.snapshot.write() = snapshot;
    }

    /// Is there demand for `source` on this thing right now?
    pub fn has_demand(&self, workspace_id: &str, thing_id: &str, source: &str) -> bool {
        self.has_demand_at(workspace_id, thing_id, source, Instant::now())
    }

    /// `has_demand` at `now` (test/simulation entry point).
    ///
    /// `source` filters explicit subscriptions only. Snapshot references and
    /// recent reads are property-level and carry no source, so they count as
    /// demand for every source of the thing — over-sampling rather than
    /// dropping demand; the sampler maps properties to sources via the
    /// catalog.
    pub fn has_demand_at(&self, workspace_id: &str, thing_id: &str, source: &str, now: Instant) -> bool {
        {
            let subscriptions = self.subscriptions.read();
            if subscriptions.values().any(|subscription| {
                subscription.workspace_id == workspace_id
                    && subscription.things.iter().any(|thing| thing == thing_id)
                    && subscription.sources.iter().any(|subscribed| subscribed == source)
                    && subscription.expires_at > now
            }) {
                return true;
            }
        }

        {
            let recent_reads = self.recent_reads.read();
            if recent_reads.iter().any(|((workspace, thing, _), at)| {
                workspace == workspace_id && thing == thing_id && now.saturating_duration_since(*at) < RECENT_READ_TTL
            }) {
                return true;
            }
        }

        self.snapshot
            .read()
            .get(&(workspace_id.to_string(), thing_id.to_string()))
            .is_some_and(|properties| !properties.is_empty())
    }

    /// Drop expired subscriptions and reads.
    pub fn gc(&self) {
        self.gc_at(Instant::now());
    }

    /// `gc` at `now` (test/simulation entry point).
    pub fn gc_at(&self, now: Instant) {
        self.subscriptions
            .write()
            .retain(|_, subscription| subscription.expires_at > now);
        self.recent_reads
            .write()
            .retain(|_, at| now.saturating_duration_since(*at) < RECENT_READ_TTL);
    }
}

impl Default for DemandTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(workspace: &str, thing: &str, properties: &[&str]) -> Vec<DemandPropertyRef> {
        properties
            .iter()
            .map(|property| DemandPropertyRef {
                workspace_id: workspace.into(),
                thing_id: thing.into(),
                property: (*property).into(),
            })
            .collect()
    }

    fn add_subscription(
        tracker: &DemandTracker,
        workspace: &str,
        things: &[&str],
        sources: &[&str],
        ttl: Duration,
        now: Instant,
    ) -> SubscriptionId {
        tracker.subscribe_at(
            workspace,
            &things.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
            &sources.iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
            ttl,
            now,
        )
    }

    #[test]
    fn empty_tracker_has_no_demand() {
        let tracker = DemandTracker::new();
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", Instant::now()));
    }

    #[test]
    fn recent_read_expires_after_ttl() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        tracker.record_read_at("ws", "t1", "temperature", base);

        assert!(tracker.has_demand_at("ws", "t1", "telemetry", base + RECENT_READ_TTL - Duration::from_secs(1)));
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", base + RECENT_READ_TTL));
        assert!(!tracker.has_demand_at("other", "t1", "telemetry", base));
        assert!(!tracker.has_demand_at("ws", "t2", "telemetry", base));
    }

    #[test]
    fn subscriptions_union_things_and_sources() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        add_subscription(&tracker, "ws", &["t1"], &["telemetry"], SUBSCRIPTION_TTL, base);
        add_subscription(&tracker, "ws", &["t1", "t2"], &["position"], SUBSCRIPTION_TTL, base);

        for (thing, source) in [("t1", "telemetry"), ("t1", "position"), ("t2", "position")] {
            assert!(tracker.has_demand_at("ws", thing, source, base));
        }
        assert!(!tracker.has_demand_at("ws", "t2", "telemetry", base));
        assert!(!tracker.has_demand_at("ws", "t1", "control", base));
        assert!(!tracker.has_demand_at("other", "t1", "telemetry", base));
    }

    #[test]
    fn snapshot_adds_demand_and_survives_expiry() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        tracker.set_snapshot(refs("ws", "t1", &["temperature"]));
        tracker.record_read_at("ws", "t1", "humidity", base);
        add_subscription(&tracker, "ws", &["t1"], &["position"], SUBSCRIPTION_TTL, base);

        // Read and subscription are both expired; the snapshot still demands.
        let late = base + Duration::from_secs(61);
        assert!(tracker.has_demand_at("ws", "t1", "telemetry", late));
        assert!(!tracker.has_demand_at("ws", "t2", "telemetry", late));
        assert!(!tracker.has_demand_at("other", "t1", "telemetry", late));

        // A new snapshot replaces the whole set.
        tracker.set_snapshot(Vec::new());
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", late));
    }

    #[test]
    fn unsubscribe_removes_only_that_subscription() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        let telemetry = add_subscription(&tracker, "ws", &["t1"], &["telemetry"], SUBSCRIPTION_TTL, base);
        add_subscription(&tracker, "ws", &["t1"], &["position"], SUBSCRIPTION_TTL, base);

        tracker.unsubscribe(&telemetry);
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", base));
        assert!(tracker.has_demand_at("ws", "t1", "position", base));

        // Unknown ids are a no-op.
        tracker.unsubscribe(&telemetry);
        tracker.unsubscribe(&9_999);
    }

    #[test]
    fn duplicate_reads_refresh_a_single_entry() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        tracker.record_read_at("ws", "t1", "temperature", base);
        tracker.record_read_at("ws", "t1", "temperature", base + Duration::from_secs(50));

        assert_eq!(tracker.recent_reads.read().len(), 1);
        // TTL runs from the second read, not the first.
        assert!(tracker.has_demand_at("ws", "t1", "telemetry", base + Duration::from_secs(61)));
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", base + Duration::from_secs(111)));
    }

    #[test]
    fn expired_subscriptions_stop_demand_and_touch_revives() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        let id = add_subscription(&tracker, "ws", &["t1"], &["telemetry"], SUBSCRIPTION_TTL, base);

        assert!(tracker.has_demand_at(
            "ws",
            "t1",
            "telemetry",
            base + SUBSCRIPTION_TTL - Duration::from_secs(1)
        ));
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", base + SUBSCRIPTION_TTL));

        assert!(tracker.touch_at(&id, base + Duration::from_secs(20)));
        assert!(tracker.has_demand_at("ws", "t1", "telemetry", base + Duration::from_secs(49)));
        assert!(!tracker.has_demand_at("ws", "t1", "telemetry", base + Duration::from_secs(50)));
        assert!(!tracker.touch_at(&9_999, base));
    }

    #[test]
    fn gc_removes_expired_subscriptions_and_reads() {
        let tracker = DemandTracker::new();
        let base = Instant::now();
        let live = add_subscription(&tracker, "ws", &["t1"], &["telemetry"], SUBSCRIPTION_TTL, base);
        add_subscription(&tracker, "ws", &["t2"], &["telemetry"], SUBSCRIPTION_TTL, base);
        tracker.record_read_at("ws", "t1", "temperature", base);

        tracker.touch_at(&live, base + Duration::from_secs(25));
        tracker.gc_at(base + Duration::from_secs(40));
        assert_eq!(tracker.subscriptions.read().len(), 1);
        assert!(tracker.subscriptions.read().contains_key(&live));
        assert_eq!(tracker.recent_reads.read().len(), 1);

        tracker.gc_at(base + Duration::from_secs(61));
        assert!(tracker.subscriptions.read().is_empty());
        assert!(tracker.recent_reads.read().is_empty());
    }

    #[test]
    fn public_wrappers_use_the_current_instant() {
        let tracker = DemandTracker::new();
        tracker.record_read("ws", "t1", "temperature");
        let id = tracker.subscribe("ws", &["t1".into()], &["telemetry".into()], SUBSCRIPTION_TTL);
        tracker.touch(&id);
        assert!(tracker.has_demand("ws", "t1", "telemetry"));
        tracker.gc();
        tracker.unsubscribe(&id);
        assert!(tracker.has_demand("ws", "t1", "telemetry")); // recent read remains
    }
}
