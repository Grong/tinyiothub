//! Db-backed adapters for the E4a access declaration ports.
//!
//! runtime stays free of db/sqlx; the composition root wraps the inherent
//! `Db` methods here (same pattern as `shared/runtime_ports.rs`).

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::driver::declaration::{AcceptanceReportRecord, DeviceDeclarationRecord, DriverDeclarationRecord};
use tinyiothub_core::driver::manifest::{ManifestTool, StateFieldDecl};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_runtime::ports::{
    AcceptanceReportStore, DeclarationProjectionStore, DemandPropertyRef, DemandSource, DeviceDeclarationStore,
    DriverDeclarationStore, ThingDeclarationSource,
};
use tinyiothub_storage::Db;

pub struct DbDriverStore(pub Arc<Db>);

#[async_trait]
impl DriverDeclarationStore for DbDriverStore {
    async fn upsert_active(&self, r: &DriverDeclarationRecord) -> Result<(), String> {
        self.0.upsert_driver_declaration(r).await.map_err(|e| e.to_string())
    }

    async fn list_active(&self, ws: &str) -> Result<Vec<DriverDeclarationRecord>, String> {
        self.0
            .list_active_driver_declarations(ws)
            .await
            .map_err(|e| e.to_string())
    }

    async fn list_all(&self) -> Result<Vec<DriverDeclarationRecord>, String> {
        self.0.list_all_driver_declarations().await.map_err(|e| e.to_string())
    }
}

pub struct DbDeviceStore(pub Arc<Db>);

#[async_trait]
impl DeviceDeclarationStore for DbDeviceStore {
    async fn upsert(&self, r: &DeviceDeclarationRecord) -> Result<(), String> {
        self.0.upsert_device_declaration(r).await.map_err(|e| e.to_string())
    }

    async fn list_by_workspace(&self, ws: &str) -> Result<Vec<DeviceDeclarationRecord>, String> {
        self.0.list_device_declarations(ws).await.map_err(|e| e.to_string())
    }

    async fn find(&self, ws: &str, thing_id: &str) -> Result<Option<DeviceDeclarationRecord>, String> {
        self.0
            .find_device_declaration(ws, thing_id)
            .await
            .map_err(|e| e.to_string())
    }
}

pub struct DbProjectionStore(pub Arc<Db>);

#[async_trait]
impl DeclarationProjectionStore for DbProjectionStore {
    async fn upsert_properties(&self, thing_id: &str, fields: &[StateFieldDecl]) -> Result<(), String> {
        self.0
            .upsert_declared_properties(thing_id, fields)
            .await
            .map_err(|e| e.to_string())
    }

    async fn upsert_commands(&self, thing_id: &str, tools: &[ManifestTool]) -> Result<(), String> {
        self.0
            .upsert_declared_commands(thing_id, tools)
            .await
            .map_err(|e| e.to_string())
    }

    async fn mark_stale(&self, thing_id: &str, keep_p: &[String], keep_c: &[String]) -> Result<(), String> {
        self.0
            .mark_stale_declared_capabilities(thing_id, keep_p, keep_c)
            .await
            .map_err(|e| e.to_string())
    }
}

pub struct DbThingSource(pub Arc<Db>);

#[async_trait]
impl ThingDeclarationSource for DbThingSource {
    async fn load_thing(&self, thing_id: &str) -> Result<Option<Thing>, String> {
        self.0
            .find_thing_with_properties(thing_id)
            .await
            .map_err(|e| e.to_string())
    }
}

/// E4b §3.2：agent crate 的 turn 快照需求出口 → 共享 DemandTracker
/// （经 AccessService，与 sampler / 读工具同一实例）。
pub struct AccessSnapshotDemandSink(pub Arc<tinyiothub_runtime::access::service::AccessService>);

impl tinyiothub_agent::runtime::thing_agent::SnapshotDemandSink for AccessSnapshotDemandSink {
    fn record_read(&self, workspace_id: &str, thing_id: &str, property: &str) {
        self.0.record_read(workspace_id, thing_id, property);
    }
}

/// E4b §2.1：持久规则（watch/派生/告警）→ sampler 需求快照。
pub struct DbDemandSource(pub Arc<Db>);

#[async_trait]
impl DemandSource for DbDemandSource {
    async fn snapshot(&self) -> Result<Vec<DemandPropertyRef>, String> {
        self.0
            .list_demand_property_refs()
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| DemandPropertyRef {
                        workspace_id: row.workspace_id,
                        thing_id: row.thing_id,
                        property: row.property,
                    })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }
}

pub struct DbAcceptanceStore(pub Arc<Db>);

#[async_trait]
impl AcceptanceReportStore for DbAcceptanceStore {
    async fn insert(&self, r: &AcceptanceReportRecord) -> Result<(), String> {
        self.0.insert_acceptance_report(r).await.map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use tinyiothub_storage::watch::{WatchRuleRow, insert_rule};

    use super::*;

    #[tokio::test]
    async fn db_demand_source_maps_rows_to_port_type() {
        let pool = tinyiothub_storage::test_helpers::test_pool().await;
        for (id, kind, condition) in [
            (
                "w-prop",
                "property",
                r#"{"type":"property","thing_id":"t-1","property":"temp","op":"gt","value":30}"#,
            ),
            (
                "w-health",
                "health",
                r#"{"type":"health","thing_id":"t-2","level":"bad"}"#,
            ),
        ] {
            insert_rule(
                &pool,
                &WatchRuleRow {
                    id: id.into(),
                    workspace_id: "ws-1".into(),
                    kind: kind.into(),
                    condition: condition.into(),
                    persistent: 0,
                    timeout_secs: 600,
                    created_at: "2026-10-21T00:00:00Z".into(),
                },
            )
            .await
            .expect("watch rule");
        }

        let source = DbDemandSource(Arc::new(Db::new(pool)));
        let snapshot = source.snapshot().await.expect("snapshot");
        assert_eq!(
            snapshot,
            vec![
                DemandPropertyRef {
                    workspace_id: "ws-1".into(),
                    thing_id: "t-1".into(),
                    property: "temp".into(),
                },
                DemandPropertyRef {
                    workspace_id: "ws-1".into(),
                    thing_id: "t-2".into(),
                    property: String::new(),
                },
            ]
        );
    }
}
