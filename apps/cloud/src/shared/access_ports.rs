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
    AcceptanceReportStore, DeclarationProjectionStore, DeviceDeclarationStore, DriverDeclarationStore,
    ThingDeclarationSource,
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

pub struct DbAcceptanceStore(pub Arc<Db>);

#[async_trait]
impl AcceptanceReportStore for DbAcceptanceStore {
    async fn insert(&self, r: &AcceptanceReportRecord) -> Result<(), String> {
        self.0.insert_acceptance_report(r).await.map_err(|e| e.to_string())
    }
}
