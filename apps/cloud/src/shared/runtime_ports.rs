//! Db-backed adapters for `tinyiothub_runtime::ports` (D15, Task 11.5).
//!
//! runtime 不依赖 db/sqlx；组合根（service_manager）在这里把
//! `tinyiothub_storage` 的具体类型包装成 runtime 端口 trait 的实现并注入。
//! 直接委托现有 storage 函数/类型，不做二次抽象（8/3 db 层模式）。

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_runtime::ports::{ThingCacheSource, ThingCommandQueries, EventRetentionStore};
use tinyiothub_storage::Db;
use tinyiothub_storage::cache::ThingCache;

/// `ThingCache` → `ThingCacheSource`（全部方法同步直转）。
pub struct DeviceCacheAdapter(pub Arc<ThingCache>);

impl ThingCacheSource for DeviceCacheAdapter {
    fn all(&self) -> Vec<Thing> {
        self.0.all()
    }

    fn get(&self, id: &str) -> Option<Thing> {
        self.0.get(id)
    }

    fn get_by_name(&self, name: &str) -> Option<Thing> {
        self.0.get_by_name(name)
    }

    fn insert(&self, device: Thing) {
        self.0.insert(device);
    }

    fn update(&self, device: Thing) {
        self.0.update(device);
    }

    fn remove(&self, id: &str) {
        self.0.remove(id);
    }
}

/// `Db` → `ThingCommandQueries`。
pub struct ThingCommandQueriesAdapter(pub Db);

#[async_trait]
impl ThingCommandQueries for ThingCommandQueriesAdapter {
    async fn find_by_thing_and_name(&self, thing_id: &str, name: &str) -> Result<Option<ThingCommand>, String> {
        self.0
            .find_thing_command_by_thing_and_name(thing_id, name)
            .await
            .map_err(|e| e.to_string())
    }
}

/// `Db` → `EventRetentionStore`。SQL 已收编进 `db::event` 领域函数（Task 10），
/// 与原 runtime 内联语句逐字一致。
pub struct EventRetentionAdapter(pub Db);

#[async_trait]
impl EventRetentionStore for EventRetentionAdapter {
    async fn delete_occurrence_events_before(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        self.0
            .delete_occurrence_events_before(cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())
    }
}
