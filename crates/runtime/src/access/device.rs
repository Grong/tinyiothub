//! In-process cache of materialized device declarations, keyed by
//! (workspace, thing). Database rows remain the source of truth; this only
//! avoids re-reading them on every call-boundary check.

use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;
use tinyiothub_core::driver::declaration::DeviceDeclaration;

#[derive(Default)]
pub struct DeviceDeclarationRegistry {
    by_thing: RwLock<HashMap<(String, String), Arc<DeviceDeclaration>>>,
}

impl DeviceDeclarationRegistry {
    pub fn upsert(&self, workspace_id: &str, thing_id: &str, decl: DeviceDeclaration) {
        self.by_thing
            .write()
            .insert((workspace_id.into(), thing_id.into()), Arc::new(decl));
    }

    pub fn get(&self, workspace_id: &str, thing_id: &str) -> Option<Arc<DeviceDeclaration>> {
        self.by_thing
            .read()
            .get(&(workspace_id.into(), thing_id.into()))
            .cloned()
    }

    pub fn remove(&self, workspace_id: &str, thing_id: &str) {
        self.by_thing.write().remove(&(workspace_id.into(), thing_id.into()));
    }
}
