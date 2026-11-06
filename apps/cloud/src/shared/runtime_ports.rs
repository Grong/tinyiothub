//! Db-backed adapters for `tinyiothub_runtime::ports` (D15, Task 11.5).
//!
//! runtime 不依赖 db/sqlx；组合根（service_manager）在这里把
//! `tinyiothub_storage` 的具体类型包装成 runtime 端口 trait 的实现并注入。
//! 直接委托现有 storage 函数/类型，不做二次抽象（8/3 db 层模式）。

use std::sync::Arc;

use async_trait::async_trait;
use tinyiothub_core::agent_task::TaskWake;
use tinyiothub_core::device_job::{DeviceJob, DeviceJobError, DeviceJobState};
use tinyiothub_core::models::spatial::{MapRecord, PlaceKind, PlaceRecord, ThingPosition};
use tinyiothub_core::models::thing::Thing;
use tinyiothub_core::models::thing_command::ThingCommand;
use tinyiothub_core::models::thing_property::ThingProperty;
use tinyiothub_runtime::ports::{
    DerivedPropertyQueries, DeviceJobStore, EventRetentionStore, MapStore, ThingCacheSource, ThingCommandQueries,
};
use tinyiothub_storage::Db;
use tinyiothub_storage::cache::ThingCache;

/// `Db` → `MediaEvidenceStore`；SQL 仅留在 db crate。
pub struct DbMediaEvidenceStore {
    db: Db,
    recorder: Option<crate::domains::agent::runtime::stream::RuntimeRecorder>,
}

impl DbMediaEvidenceStore {
    pub fn new(db: Db) -> Self {
        Self { db, recorder: None }
    }

    pub fn with_recorder(mut self, recorder: crate::domains::agent::runtime::stream::RuntimeRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }
}

#[async_trait]
impl tinyiothub_runtime::ports::MediaEvidenceStore for DbMediaEvidenceStore {
    async fn insert_or_get(
        &self,
        row: &tinyiothub_core::media::MediaEvidenceRecord,
    ) -> Result<tinyiothub_core::media::MediaEvidenceRecord, tinyiothub_core::media::MediaError> {
        let (stored, event) = match self.db.put_media_evidence_and_event(row).await {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    metric = "runtime_evidence_failure",
                    workspace_id = %row.workspace_id,
                    device_id = %row.device_id,
                    mime = %row.mime,
                    %error,
                    "media evidence persistence failed"
                );
                return Err(error);
            }
        };
        tracing::info!(
            metric = "runtime_evidence_stored",
            workspace_id = %stored.workspace_id,
            evidence_id = %stored.id,
            device_id = %stored.device_id,
            size_bytes = stored.size_bytes,
            mime = %stored.mime,
            "media evidence stored"
        );
        if let (Some(recorder), Some(event)) = (&self.recorder, event) {
            recorder.publish(event);
        }
        Ok(stored)
    }

    async fn get(
        &self,
        evidence_id: &str,
    ) -> Result<Option<tinyiothub_core::media::MediaEvidenceRecord>, tinyiothub_core::media::MediaError> {
        self.db.get_media_evidence(evidence_id).await
    }

    async fn delete_expired(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<tinyiothub_core::media::MediaEvidenceRecord>, tinyiothub_core::media::MediaError> {
        let deleted = self.db.delete_expired_media(now).await?;
        if !deleted.is_empty() {
            tracing::info!(
                metric = "runtime_evidence_expired",
                count = deleted.len(),
                bytes = deleted.iter().map(|row| row.size_bytes).sum::<u64>(),
                "expired media evidence removed"
            );
        }
        Ok(deleted)
    }

    async fn storage_is_referenced(&self, path: &str) -> Result<bool, tinyiothub_core::media::MediaError> {
        self.db.media_storage_is_referenced(path).await
    }
}

pub struct ThingCacheAdapter(pub Arc<ThingCache>);

impl ThingCacheSource for ThingCacheAdapter {
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

/// Legacy platform `DeviceJobStore` — test-only parity reference.
///
/// Task 19C3 moved every production `DeviceJobStore` operation to
/// [`WorkspaceDeviceJobStore`]. This adapter still writes the platform
/// `device_jobs` table (and its test-only runtime events) so unit tests can
/// compare legacy semantics against the workspace store on the same fixtures.
/// It is not compiled into the production binary.
#[cfg(test)]
pub struct DbDeviceJobStore {
    db: Db,
    recorder: Option<crate::domains::agent::runtime::stream::RuntimeRecorder>,
}

#[cfg(test)]
impl DbDeviceJobStore {
    pub fn new(db: Db) -> Self {
        Self { db, recorder: None }
    }

    pub fn with_recorder(mut self, recorder: crate::domains::agent::runtime::stream::RuntimeRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    fn publish(&self, event: tinyiothub_storage::runtime_event::RuntimeEvent) {
        if let Some(recorder) = &self.recorder {
            recorder.publish(event);
        }
    }
}

#[cfg(test)]
#[async_trait]
impl DeviceJobStore for DbDeviceJobStore {
    async fn create_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError> {
        let event = self.db.create_job_and_event(job).await?;
        self.publish(event);
        Ok(())
    }

    async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        self.db.get_job(workspace_id, job_id).await
    }

    async fn get_job_by_id(&self, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        self.db.get_job_by_id(job_id).await
    }

    async fn get_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        self.db.get_task_job(workspace_id, task_id, job_id).await
    }

    async fn get_job_by_tool_call(
        &self,
        workspace_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        self.db.get_job_by_tool_call(workspace_id, tool_call_id).await
    }

    async fn submit_approved_job(&self, job: &DeviceJob) -> Result<(Option<DeviceJob>, bool), DeviceJobError> {
        if let Some(tool_call_id) = job.tool_call_id.as_deref()
            && let Some(existing) = self.db.get_job_by_tool_call(&job.workspace_id, tool_call_id).await?
        {
            let matches = existing.device_id == job.device_id
                && existing.tool == job.tool
                && existing.args_json == job.args_json
                && existing.task_id == job.task_id
                && existing.run_id == job.run_id;
            if !matches {
                return Err(DeviceJobError::ExecutionConflict);
            }
            return Ok((Some(existing), false));
        }
        self.create_job(job).await?;
        Ok((Some(job.clone()), true))
    }

    async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError> {
        let (job, event) = self.db.mark_running_and_event(job_id, driver_call_id).await?;
        self.publish(event);
        Ok(job)
    }

    async fn finish_job(
        &self,
        job_id: &str,
        state: DeviceJobState,
        result: &serde_json::Value,
        wake: Option<&TaskWake>,
    ) -> Result<bool, DeviceJobError> {
        let (finished, event) = self.db.finish_job_and_event(job_id, state, result, wake).await?;
        if let Some(event) = event {
            self.publish(event);
        }
        Ok(finished)
    }

    async fn update_progress(&self, job_id: &str, progress: &serde_json::Value) -> Result<bool, DeviceJobError> {
        let (revision, event) = self.db.update_progress_revision_and_event(job_id, progress).await?;
        if let Some(event) = event {
            self.publish(event);
        }
        Ok(revision.is_some())
    }

    async fn request_stop(&self, workspace_id: &str, task_id: &str, job_id: &str) -> Result<DeviceJob, DeviceJobError> {
        let (job, event) = self.db.request_stop_and_event(workspace_id, task_id, job_id).await?;
        self.publish(event);
        Ok(job)
    }

    async fn active_jobs(
        &self,
        workspace_id: Option<&str>,
        unknown_only: bool,
    ) -> Result<Vec<DeviceJob>, DeviceJobError> {
        self.db.active_jobs(workspace_id, unknown_only).await
    }
}

/// `DeviceJobStore` backed by the owning workspace Harness generation.
///
/// Task 19C3 made the workspace `device_jobs` the only Job truth: every
/// operation reads or writes the Job's workspace, and no Job id prefix is
/// special. Job-id-only operations resolve the owner by scanning the registered
/// Harness generations once and caching the result; point reads fail closed
/// when an unreadable workspace could own the Job (Task 19A N1).
pub struct WorkspaceDeviceJobStore {
    db: Arc<Db>,
    workspaces: Arc<dashmap::DashMap<String, String>>,
}

impl WorkspaceDeviceJobStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            workspaces: Arc::new(dashmap::DashMap::new()),
        }
    }

    async fn workspace_for_job(&self, job_id: &str, mutating: bool) -> Result<Option<String>, DeviceJobError> {
        if let Some(workspace_id) = self.workspaces.get(job_id) {
            return Ok(Some(workspace_id.clone()));
        }
        let workspace_id = self.db.find_workspace_for_job(job_id, mutating).await?;
        if let Some(workspace_id) = &workspace_id {
            self.workspaces.insert(job_id.to_string(), workspace_id.clone());
        }
        Ok(workspace_id)
    }
}

#[async_trait]
impl DeviceJobStore for WorkspaceDeviceJobStore {
    async fn create_job(&self, job: &DeviceJob) -> Result<(), DeviceJobError> {
        self.db.create_workspace_job(job).await?;
        self.workspaces.insert(job.id.clone(), job.workspace_id.clone());
        Ok(())
    }

    async fn get_job(&self, workspace_id: &str, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        let job = self.db.get_workspace_job(workspace_id, job_id).await?;
        if job.is_some() {
            self.workspaces.insert(job_id.to_string(), workspace_id.to_string());
        }
        Ok(job)
    }

    async fn get_job_by_id(&self, job_id: &str) -> Result<Option<DeviceJob>, DeviceJobError> {
        let Some(workspace_id) = self.workspace_for_job(job_id, false).await? else {
            return Ok(None);
        };
        self.db.get_workspace_job(&workspace_id, job_id).await
    }

    async fn get_task_job(
        &self,
        workspace_id: &str,
        task_id: &str,
        job_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        let job = self.db.get_workspace_task_job(workspace_id, task_id, job_id).await?;
        if job.is_some() {
            self.workspaces.insert(job_id.to_string(), workspace_id.to_string());
        }
        Ok(job)
    }

    async fn get_job_by_tool_call(
        &self,
        workspace_id: &str,
        tool_call_id: &str,
    ) -> Result<Option<DeviceJob>, DeviceJobError> {
        let job = self
            .db
            .get_workspace_job_by_tool_call(workspace_id, tool_call_id)
            .await?;
        if let Some(job) = &job {
            self.workspaces.insert(job.id.clone(), workspace_id.to_string());
        }
        Ok(job)
    }

    async fn submit_approved_job(&self, job: &DeviceJob) -> Result<(Option<DeviceJob>, bool), DeviceJobError> {
        if let Some(tool_call_id) = job.tool_call_id.as_deref()
            && let Some(existing) = self
                .db
                .get_workspace_job_by_tool_call(&job.workspace_id, tool_call_id)
                .await?
        {
            let matches = existing.device_id == job.device_id
                && existing.tool == job.tool
                && existing.args_json == job.args_json
                && existing.task_id == job.task_id
                && existing.run_id == job.run_id;
            if !matches {
                return Err(DeviceJobError::ExecutionConflict);
            }
            self.workspaces
                .insert(existing.id.clone(), existing.workspace_id.clone());
            return Ok((Some(existing), false));
        }
        self.create_job(job).await?;
        Ok((Some(job.clone()), true))
    }

    async fn mark_running(&self, job_id: &str, driver_call_id: &str) -> Result<DeviceJob, DeviceJobError> {
        let Some(workspace_id) = self.workspace_for_job(job_id, true).await? else {
            return Err(DeviceJobError::NotFound);
        };
        self.db
            .mark_workspace_job_running(&workspace_id, job_id, driver_call_id)
            .await
    }

    async fn finish_job(
        &self,
        job_id: &str,
        state: DeviceJobState,
        result: &serde_json::Value,
        wake: Option<&TaskWake>,
    ) -> Result<bool, DeviceJobError> {
        let Some(workspace_id) = self.workspace_for_job(job_id, true).await? else {
            return Err(DeviceJobError::NotFound);
        };
        self.db
            .finish_workspace_job(&workspace_id, job_id, state, result, wake)
            .await
    }

    async fn update_progress(&self, job_id: &str, progress: &serde_json::Value) -> Result<bool, DeviceJobError> {
        let Some(workspace_id) = self.workspace_for_job(job_id, true).await? else {
            return Err(DeviceJobError::NotFound);
        };
        self.db
            .update_workspace_job_progress(&workspace_id, job_id, progress)
            .await
    }

    async fn request_stop(&self, workspace_id: &str, task_id: &str, job_id: &str) -> Result<DeviceJob, DeviceJobError> {
        let job = self
            .db
            .request_workspace_job_stop(workspace_id, task_id, job_id)
            .await?;
        self.workspaces.insert(job.id.clone(), workspace_id.to_string());
        Ok(job)
    }

    async fn active_jobs(
        &self,
        workspace_id: Option<&str>,
        unknown_only: bool,
    ) -> Result<Vec<DeviceJob>, DeviceJobError> {
        let jobs = self.db.active_workspace_jobs(workspace_id, unknown_only).await?;
        for job in &jobs {
            self.workspaces.insert(job.id.clone(), job.workspace_id.clone());
        }
        Ok(jobs)
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

/// `Db` → `DerivedPropertyQueries`（spec §三 E1：派生驱动管理器的启动索引
/// 重建与 descendant_count 求值经此端口读库）。
pub struct DbDerivedPropertyQueries(pub Db);

#[async_trait]
impl DerivedPropertyQueries for DbDerivedPropertyQueries {
    async fn list_derived(&self) -> Result<Vec<ThingProperty>, String> {
        self.0.find_derived_properties().await.map_err(|e| e.to_string())
    }

    async fn find_thing_with_properties(&self, thing_id: &str) -> Result<Option<Thing>, String> {
        // 既有 find 组合（ensure_owner 热路径自愈，Task 13 评审 Important-1）：
        // find_thing_by_id(None,…) 不做 workspace 过滤（钩子已鉴权，id 全局唯一）。
        let mut thing = match self
            .0
            .find_thing_by_id(None, thing_id)
            .await
            .map_err(|e| e.to_string())?
        {
            Some(t) => t,
            None => return Ok(None),
        };
        let props = self
            .0
            .find_thing_properties_by_thing_id(thing_id)
            .await
            .map_err(|e| e.to_string())?;
        thing.properties = Some(props);
        Ok(Some(thing))
    }

    async fn count_descendants(&self, root_thing_id: &str, thing_type: Option<&str>) -> Result<i64, String> {
        // 递归 CTE，深度上限 10（spec §二 防环兜底）；SQL 住 crates/db。
        tinyiothub_storage::thing::count_descendants(self.0.pool(), root_thing_id, thing_type)
            .await
            .map_err(|e| e.to_string())
    }
}

/// E4d：`Db` → `MapStore`（ZoneService 的 maps/places/position 读面）。
/// runtime 仍零 db；workspace 隔离由 db 层 SQL 保证。
pub struct DbMapStore(pub Db);

#[async_trait]
impl MapStore for DbMapStore {
    async fn list_maps(&self, workspace_id: &str) -> Result<Vec<MapRecord>, String> {
        self.0.list_maps(workspace_id).await.map_err(|e| e.to_string())
    }

    async fn default_map(&self, workspace_id: &str) -> Result<Option<MapRecord>, String> {
        self.0.find_default_map(workspace_id).await.map_err(|e| e.to_string())
    }

    async fn upsert_map(&self, map: &MapRecord) -> Result<(), String> {
        self.0.upsert_map(map).await.map_err(|e| e.to_string())
    }

    async fn delete_map(&self, workspace_id: &str, id: &str) -> Result<bool, String> {
        self.0.delete_map(workspace_id, id).await.map_err(|e| e.to_string())
    }

    async fn list_places(&self, workspace_id: &str, map_id: &str) -> Result<Vec<PlaceRecord>, String> {
        self.0
            .list_places(workspace_id, map_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn find_place_by_name(
        &self,
        workspace_id: &str,
        name: &str,
        kind: Option<PlaceKind>,
    ) -> Result<Option<PlaceRecord>, String> {
        self.0
            .find_place_by_name(workspace_id, name, kind)
            .await
            .map_err(|e| e.to_string())
    }

    async fn upsert_place(&self, workspace_id: &str, place: &PlaceRecord) -> Result<(), String> {
        self.0
            .upsert_place(workspace_id, place)
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_place(&self, workspace_id: &str, map_id: &str, id: &str) -> Result<bool, String> {
        self.0
            .delete_place(workspace_id, map_id, id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_position(&self, workspace_id: &str, thing_id: &str) -> Result<Option<ThingPosition>, String> {
        self.0
            .get_thing_position(workspace_id, thing_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn set_position(&self, workspace_id: &str, thing_id: &str, position: &ThingPosition) -> Result<(), String> {
        self.0
            .set_thing_position(workspace_id, thing_id, position)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn set_reading_position(
        &self,
        workspace_id: &str,
        thing_id: &str,
        position: &ThingPosition,
    ) -> Result<(), String> {
        self.0
            .set_thing_reading_position(workspace_id, thing_id, position)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn list_positions(&self, workspace_id: &str) -> Result<Vec<(String, ThingPosition)>, String> {
        self.0
            .list_thing_positions(workspace_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_zone_ref(&self, workspace_id: &str, thing_id: &str) -> Result<Option<String>, String> {
        self.0
            .get_thing_zone_ref(workspace_id, thing_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn set_zone_ref(&self, workspace_id: &str, thing_id: &str, zone_ref: Option<&str>) -> Result<(), String> {
        self.0
            .set_thing_zone_ref(workspace_id, thing_id, zone_ref)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn list_zone_refs(&self, workspace_id: &str) -> Result<Vec<(String, String)>, String> {
        self.0
            .list_thing_zone_refs(workspace_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_position_ingest(&self, workspace_id: &str, thing_id: &str) -> Result<bool, String> {
        self.0
            .get_thing_position_ingest(workspace_id, thing_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn set_position_ingest(&self, workspace_id: &str, thing_id: &str, ingest: bool) -> Result<(), String> {
        self.0
            .set_thing_position_ingest(workspace_id, thing_id, ingest)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// 缺失/未知归一化为 `hierarchy`（迁移期回退，与端口文档一致）。
    async fn zone_source(&self, workspace_id: &str) -> Result<String, String> {
        Ok(self
            .0
            .get_zone_source(workspace_id)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "hierarchy".to_string()))
    }
}

/// T6：approval_timeout 执行器的 db 适配器。超时判断逐条：awaiting_approval
/// → escalated（条件迁移）→ create_escalation 工单 → 关联。
///
/// 2026-09-14 硬化（E2/2-1A）：三态 SLA + 逐条失败隔离（单条失败记 error
/// 继续扫，不株连整轮——一条坏数据不能反复阻断全部清扫）。
pub struct ApprovalTimeoutAdapter {
    pub db: Db,
    pub sse: Arc<crate::domains::event::sse_manager::SseConnectionManager>,
    /// 对账恢复用：stale investigating 且存在已完成 run 时重投影路由
    /// （2026-09-16 实测：run 完成但事件丢失，judgment 永久卡死）。
    pub alarm_service: Arc<crate::domains::alarm::service::AlarmService>,
    /// X1：SLA 升级（审批/执行超时转工单）时的「需要你」通知出口；None 静默。
    pub notify: Option<Arc<crate::domains::notify::NotificationManager>>,
}

impl ApprovalTimeoutAdapter {
    /// 从已完成的调查 run 重投影判断（事件丢失的对账兜底）。judgment 滞留
    /// investigating 但其 alarm 键已有完成 run 时，把存储的 RunReport 重新
    /// 走一遍 subscriber 路由（条件迁移幂等）。返回 true = 已恢复路由。
    async fn reproject_from_completed_run(&self, judgment: &tinyiothub_storage::judgment::Judgment) -> bool {
        let (Some(thing_id), Some(alarm_id)) = (judgment.thing_id.as_deref(), judgment.alarm_id.as_deref()) else {
            return false;
        };
        let rule_id = match self.db.find_alarm_by_id(alarm_id, Some(&judgment.workspace_id)).await {
            Ok(Some(a)) => a.rule_id.clone(),
            Ok(None) => {
                tracing::warn!(judgment_id = %judgment.id, alarm_id, "reproject: alarm row missing");
                return false;
            }
            Err(e) => {
                tracing::warn!(judgment_id = %judgment.id, error = %e, "reproject: alarm load failed");
                return false;
            }
        };
        let problem_key = format!("alarm:{}:{}", thing_id, rule_id.as_deref().unwrap_or("-"));
        let report = match self
            .db
            .latest_agent_run_report_by_problem_key(&judgment.workspace_id, &problem_key)
            .await
        {
            Ok(Some(r)) => r,
            Ok(None) => return false, // 没有已完成的 run——真的没被受理
            Err(e) => {
                tracing::warn!(judgment_id = %judgment.id, error = %e, "reproject: run report load failed");
                return false;
            }
        };
        tracing::info!(judgment_id = %judgment.id, problem_key, run_id = %report.run_id,
            "reprojecting stale judgment from completed run (RunRecorded event was lost)");
        let event = crate::domains::agent::events::AgentEvent {
            seq: 0,
            occurred_at: chrono::Utc::now(),
            kind: crate::domains::agent::events::AgentEventKind::RunRecorded {
                report: Box::new(report),
                problem_key: Some(problem_key),
                dedup_key: None,
            },
        };
        crate::domains::agent::host::judge::judgment_subscriber::project(
            &event,
            &self.db,
            &self.sse,
            &self.alarm_service,
            &self.notify,
        )
        .await;
        // project 走条件迁移——确认判断真的离开了 investigating
        matches!(
            self.db.find_judgment_by_id(&judgment.id, &judgment.workspace_id).await,
            Ok(Some(j)) if j.status != tinyiothub_storage::judgment::JudgmentStatus::Investigating
        )
    }

    /// 升级一条判断为工单（共用 awaiting_approval/executing 两路）。
    async fn escalate_one(
        &self,
        judgment: &tinyiothub_storage::judgment::Judgment,
        from: tinyiothub_storage::judgment::JudgmentStatus,
        title_prefix: &str,
        source: &str,
    ) -> Result<(), String> {
        let transitioned = self
            .db
            .transit_judgment(
                &judgment.id,
                from,
                tinyiothub_storage::judgment::JudgmentStatus::Escalated,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        if !transitioned {
            return Ok(()); // 并发互撞（人刚好在批/执行刚好完成）→ 跳过
        }
        let ticket_id = crate::domains::ticket::create_escalation(
            &self.db,
            &self.sse,
            crate::domains::ticket::Escalation {
                workspace_id: judgment.workspace_id.clone(),
                thing_id: judgment.thing_id.clone(),
                agent_run_id: judgment
                    .run_id
                    .clone()
                    .unwrap_or_else(|| format!("{source}:{}", judgment.id)),
                title: format!(
                    "{title_prefix}：{}",
                    judgment.reason.chars().take(70).collect::<String>()
                ),
                briefing: serde_json::json!({
                    "problem": judgment.reason,
                    "source": source,
                    "judgment_id": judgment.id,
                    "alarm_id": judgment.alarm_id,
                    "suggested_action": judgment.suggested_action,
                }),
                failure_hash: format!("pk:{source}:{}", judgment.id),
            },
        )
        .await;
        match ticket_id {
            Some(tid) => {
                if let Err(e) = self.db.link_judgment_ticket(&judgment.id, tid).await {
                    tracing::warn!(judgment_id = %judgment.id, error = %e, "link ticket failed");
                }
                // X1：SLA 升级也是「需要你」事件——通知到人（best-effort）。
                if let Some(manager) = &self.notify {
                    let message = crate::domains::notify::NotificationMessage::new(
                        "需要你：AI 已转工单".to_string(),
                        format!("{}（工单 #{}）", title_prefix, tid),
                        tinyiothub_core::models::event::EventLevel::Error,
                        vec![tinyiothub_core::notification_types::NotificationChannelType::Sse],
                        vec![judgment.workspace_id.clone()],
                    );
                    let manager = manager.clone();
                    let judgment_id = judgment.id.clone();
                    tokio::spawn(async move {
                        if let Err(e) = manager.send_notification(&message).await {
                            tracing::warn!(judgment_id, error = %e, "needs-you notify dispatch failed (best-effort)");
                        }
                    });
                }
                Ok(())
            }
            // 票建失败：judgment 已 escalated 但无工单——不计入升级数并 error
            // （该判断此后不再被 SLA 命中，必须响亮）
            None => Err(format!(
                "judgment {} escalated but ticket creation failed (orphan escalated state)",
                judgment.id
            )),
        }
    }
}

#[async_trait]
impl tinyiothub_runtime::ports::ApprovalTimeoutStore for ApprovalTimeoutAdapter {
    async fn escalate_stale_approvals(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_awaiting_approvals(cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())?;
        let mut escalated = 0u64;
        for judgment in stale {
            // 逐条失败隔离（2-1A）：单条失败记 error 继续，不中断整轮
            if let Err(e) = self
                .escalate_one(
                    &judgment,
                    tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
                    "审批超时",
                    "approval-timeout",
                )
                .await
            {
                tracing::error!(judgment_id = %judgment.id, error = %e, "approval-timeout escalation failed (isolated)");
                continue;
            }
            escalated += 1;
        }
        Ok(escalated)
    }

    /// investigating 超 SLA → 先对账：存在已完成的调查 run 时重投影路由
    /// （RunRecorded 事件丢失的兜底——2026-09-16 实测 run 完成、verdict
    /// 合法，但 judgment 永久卡 investigating）；无 run 才标
    /// dispatch_suppressed（不开票不计预算；迟到 RunRecorded 由 subscriber
    /// 恢复路由，T-7/L1）。
    async fn mark_stale_investigating(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_by_status(
                tinyiothub_storage::judgment::JudgmentStatus::Investigating,
                cutoff_rfc3339,
            )
            .await
            .map_err(|e| e.to_string())?;
        let mut marked = 0u64;
        for judgment in stale {
            if self.reproject_from_completed_run(&judgment).await {
                continue; // 已从完成的 run 恢复路由，不再是滞留判断
            }
            match self
                .db
                .fail_judgment(
                    &judgment.id,
                    tinyiothub_storage::judgment::JudgmentStatus::DispatchSuppressed,
                    "调查未受理或超时（防抖/队列），已标记",
                )
                .await
            {
                Ok(true) => marked += 1,
                Ok(false) => {} // 并发迁移（刚好判出）→ 跳过
                Err(e) => {
                    tracing::error!(judgment_id = %judgment.id, error = %e, "mark dispatch_suppressed failed (isolated)")
                }
            }
        }
        Ok(marked)
    }

    /// executing 超 SLA → escalated + 人工确认工单（T-16/C4：覆盖
    /// Acted-未验证、NoActionNeeded-冲突、exec 挂起三种「执行未闭环」）。
    async fn escalate_stale_executing(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_by_status(tinyiothub_storage::judgment::JudgmentStatus::Executing, cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())?;
        let mut escalated = 0u64;
        for judgment in stale {
            if let Err(e) = self
                .escalate_one(
                    &judgment,
                    tinyiothub_storage::judgment::JudgmentStatus::Executing,
                    "执行超时/未验证，需人工确认",
                    "execution-timeout",
                )
                .await
            {
                tracing::error!(judgment_id = %judgment.id, error = %e, "execution-timeout escalation failed (isolated)");
                continue;
            }
            escalated += 1;
        }
        Ok(escalated)
    }

    /// X5：dismissed 超 cutoff 且关联报警仍 Active → 重浮 investigating 并重派
    /// 调查（否决有兜底——「用户说不用处理」不能成为永久静默）。报警已消/无
    /// 报警上下文的保持 dismissed（用户对了一半：事已经过去了）。
    async fn resurface_stale_dismissed(&self, cutoff_rfc3339: &str) -> Result<u64, String> {
        let stale = self
            .db
            .stale_by_status(tinyiothub_storage::judgment::JudgmentStatus::Dismissed, cutoff_rfc3339)
            .await
            .map_err(|e| e.to_string())?;
        let mut resurfaced = 0u64;
        for judgment in stale {
            let active_alarm = match &judgment.alarm_id {
                Some(alarm_id) => matches!(
                    self.alarm_service
                        .get_alarm_by_id(alarm_id, Some(&judgment.workspace_id))
                        .await,
                    Ok(Some(alarm)) if alarm.status.is_active()
                ),
                None => false,
            };
            if !active_alarm {
                continue;
            }
            // R-F1 守卫：同报警已有未终态判断（dismissed 不占 active 索引，
            // 新报警可合法触发新判断）→ 新判断才是活的，旧行保持 dismissed，
            // 不撞 judgments_active_alarm 唯一索引（每周期 error log 消除）。
            // 已知残窗（check-then-act，外部声音 #2）：本查询与下方 transit 非
            // 同事务，并发 insert_judgment 仍可能撞索引——主路径（存量 dismissed
            // 行的周期重试噪声）已消除，残窗以 error log 形式残留可接受。
            if let Some(alarm_id) = &judgment.alarm_id {
                match self.db.has_open_judgment_for_alarm(alarm_id).await {
                    Ok(true) => {
                        tracing::debug!(judgment_id = %judgment.id, alarm_id, "resurface skipped: newer open judgment owns the alarm");
                        continue;
                    }
                    Ok(false) => {}
                    Err(e) => {
                        tracing::warn!(judgment_id = %judgment.id, error = %e, "open-judgment guard query failed, skipping resurface this cycle");
                        continue;
                    }
                }
            }
            match self
                .db
                .transit_judgment(
                    &judgment.id,
                    tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
                    tinyiothub_storage::judgment::JudgmentStatus::Investigating,
                    None,
                )
                .await
            {
                Ok(true) => {
                    // 重派调查（boot race 同机制：subscriber 按 thing+rule 匹配既有
                    // judgment 不重复建行）；失败只记日志——investigating SLA 兜底。
                    self.alarm_service.redispatch_judgment_investigation(&judgment).await;
                    resurfaced += 1;
                }
                Ok(false) => {} // 并发互撞（刚好被其他路径迁移）→ 跳过
                Err(e) => {
                    tracing::error!(judgment_id = %judgment.id, error = %e, "resurface dismissed failed (isolated)");
                    continue;
                }
            }
        }
        Ok(resurfaced)
    }
}

/// `Db` → `HubLedger`（E1 spec §4.2）：账本只增不改，ts 由适配器落 RFC3339，
/// payload 序列化为 JSON 文本（词表版本见 payload.v，P5）。
///
/// F4 兜底观测：append 成功路径按 (workspace, UTC 日) 进程内计数，日增越过
/// [`HUB_LEDGER_DAILY_WARN_THRESHOLD`] 当日 warn 一次（metric=
/// "hub_ledger_volume"）。进程内计数重启清零——兜底告警非精确计量（精确
/// 体量由运维慢查 SQL 兜底），观测不影响落账成败。
pub struct HubLedgerAdapter {
    db: Db,
    /// (workspace_id, "YYYY-MM-DD") → 本进程当日成功落账行数。
    daily_counts: dashmap::DashMap<(String, String), u64>,
}

/// F4 兜底：hub_ledger 单工作区日增行数告警阈值。
const HUB_LEDGER_DAILY_WARN_THRESHOLD: u64 = 100_000;

impl HubLedgerAdapter {
    pub fn new(db: Db) -> Self {
        Self {
            db,
            daily_counts: dashmap::DashMap::new(),
        }
    }

    /// 落账成功后计数：返回 Some((day, count)) = 本日首次越阈（调用方 warn 一次）。
    fn note_appended(&self, workspace_id: &str) -> Option<(String, u64)> {
        let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut entry = self
            .daily_counts
            .entry((workspace_id.to_string(), day.clone()))
            .or_insert(0);
        *entry += 1;
        let count = *entry;
        (count == HUB_LEDGER_DAILY_WARN_THRESHOLD).then_some((day, count))
    }
}

#[async_trait]
impl tinyiothub_runtime::ports::HubLedger for HubLedgerAdapter {
    async fn append(
        &self,
        workspace_id: &str,
        kind: &str,
        thing_id: Option<&str>,
        payload: serde_json::Value,
    ) -> Result<(), String> {
        let ts = chrono::Utc::now().to_rfc3339();
        let payload = serde_json::to_string(&payload).map_err(|e| e.to_string())?;
        tinyiothub_storage::watch::append_ledger(self.db.pool(), workspace_id, &ts, kind, thing_id, &payload)
            .await
            .map_err(|e| e.to_string())?;
        if let Some((day, count)) = self.note_appended(workspace_id) {
            tracing::warn!(
                metric = "hub_ledger_volume",
                workspace_id,
                %day,
                count,
                "hub_ledger 日增行数越阈（F4 兜底观测）"
            );
        }
        Ok(())
    }
}

/// `Db` → `WatchRuleStore`（E1 spec §4.1）：kind 列存 condition 的 serde type
/// tag（property|health|availability），condition 列存完整 condition JSON；
/// 读回按 JSON 反序列化重构 `WatchCondition`（core 的 WatchRule 无 serde
/// derive，按字段映射，不回改 core）。
pub struct WatchRuleStoreAdapter {
    db: Db,
    recorder: Option<crate::domains::agent::runtime::stream::RuntimeRecorder>,
}

impl WatchRuleStoreAdapter {
    pub fn new(db: Db) -> Self {
        Self { db, recorder: None }
    }

    pub fn with_recorder(mut self, recorder: crate::domains::agent::runtime::stream::RuntimeRecorder) -> Self {
        self.recorder = Some(recorder);
        self
    }

    fn publish(&self, event: tinyiothub_storage::runtime_event::RuntimeEvent) {
        if let Some(recorder) = &self.recorder {
            recorder.publish(event);
        }
    }
}

#[async_trait]
impl tinyiothub_runtime::ports::WatchRuleStore for WatchRuleStoreAdapter {
    async fn list_all(&self) -> Result<Vec<tinyiothub_core::watch::WatchRule>, String> {
        let rows = tinyiothub_storage::watch::list_rules(self.db.pool())
            .await
            .map_err(|e| e.to_string())?;
        let task_workspaces: std::collections::HashMap<String, String> = self
            .db
            .list_nonterminal_tasks(None)
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|task| (task.id, task.workspace_id))
            .collect();
        rows.iter()
            .filter(|row| {
                row.owner_task_id
                    .as_ref()
                    .is_none_or(|task_id| task_workspaces.get(task_id) == Some(&row.workspace_id))
            })
            .map(|row| {
                let condition: tinyiothub_core::watch::WatchCondition = serde_json::from_str(&row.condition)
                    .map_err(|e| format!("watch_rule {} condition JSON 损坏: {e}", row.id))?;
                Ok(tinyiothub_core::watch::WatchRule {
                    id: row.id.clone(),
                    workspace_id: row.workspace_id.clone(),
                    condition,
                    persistent: row.persistent != 0,
                    timeout_secs: row.timeout_secs as u32,
                    created_at: row.created_at.clone(),
                    owner_task_id: row.owner_task_id.clone(),
                })
            })
            .collect()
    }

    async fn insert(&self, rule: &tinyiothub_core::watch::WatchRule) -> Result<(), String> {
        if let Some(task_id) = rule.owner_task_id.as_deref() {
            let task = self
                .db
                .get_task(&rule.workspace_id, task_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "watch owner Task does not exist in this workspace".to_string())?;
            if task.status.is_terminal() {
                return Err("watch owner Task is terminal".to_string());
            }
        }
        let condition = serde_json::to_value(&rule.condition).map_err(|e| e.to_string())?;
        let kind = condition
            .get("type")
            .and_then(|t| t.as_str())
            .ok_or_else(|| "WatchCondition 序列化缺 type tag".to_string())?
            .to_string();
        let row = tinyiothub_storage::watch::WatchRuleRow {
            id: rule.id.clone(),
            workspace_id: rule.workspace_id.clone(),
            kind,
            condition: serde_json::to_string(&condition).map_err(|e| e.to_string())?,
            persistent: i64::from(rule.persistent),
            timeout_secs: i64::from(rule.timeout_secs),
            created_at: rule.created_at.clone(),
            owner_task_id: rule.owner_task_id.clone(),
        };
        let event = tinyiothub_storage::watch::insert_rule_and_event(self.db.pool(), &row)
            .await
            .map_err(|e| e.to_string())?;
        self.publish(event);
        Ok(())
    }

    async fn delete(&self, workspace_id: &str, id: &str) -> Result<bool, String> {
        let event = tinyiothub_storage::watch::delete_rule_and_event(self.db.pool(), workspace_id, id)
            .await
            .map_err(|e| e.to_string())?;
        let deleted = event.is_some();
        if let Some(event) = event {
            self.publish(event);
        }
        Ok(deleted)
    }

    async fn complete_task_watch(
        &self,
        workspace_id: &str,
        id: &str,
        wake: &tinyiothub_core::agent_task::TaskWake,
    ) -> Result<bool, String> {
        let (completed, event) =
            tinyiothub_storage::watch::complete_task_watch_and_event(self.db.pool(), workspace_id, id, wake)
                .await
                .map_err(|e| e.to_string())?;
        if let Some(event) = event {
            self.publish(event);
        }
        Ok(completed)
    }

    async fn delete_task_watches(&self, workspace_id: &str, task_id: &str) -> Result<usize, String> {
        let events = tinyiothub_storage::watch::delete_task_watches_and_events(self.db.pool(), workspace_id, task_id)
            .await
            .map_err(|e| e.to_string())?;
        let count = events.len();
        for event in events {
            self.publish(event);
        }
        Ok(count)
    }
}

#[cfg(test)]
mod runtime_job_event_tests {
    use super::*;
    use tinyiothub_runtime::ports::{DeviceJobStore, MediaEvidenceStore, WatchRuleStore};

    async fn fixture() -> (
        Arc<Db>,
        crate::domains::agent::runtime::stream::RuntimeRecorder,
        Arc<crate::domains::agent::runtime::stream::RuntimeStreamHub>,
    ) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.expect("pool");
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .expect("migrations");
        let db = Arc::new(Db::new(pool));
        tinyiothub_storage::seed::seed_system(&db).await.expect("seed");
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws-job','ws-job','tenant-default-001','2025-01-01','2025-01-01')")
            .execute(db.pool())
            .await
            .expect("workspace");
        let now = chrono::Utc::now();
        db.create_task_with_wake(
            &tinyiothub_core::agent_task::AgentTask {
                id: "task-1".into(),
                workspace_id: "ws-job".into(),
                idempotency_key: "job-event".into(),
                objective: "inspect".into(),
                session_key: None,
                created_by: "user-1".into(),
                status: tinyiothub_core::agent_task::TaskStatus::Active,
                current_run_id: None,
                state_version: 0,
                created_at: now,
                updated_at: now,
                ended_at: None,
            },
            &tinyiothub_core::agent_task::TaskWake {
                task_id: "task-1".into(),
                event_id: "job-event-create".into(),
                signal_json: serde_json::json!({"workspace_id":"ws-job"}).to_string(),
                dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            },
        )
        .await
        .expect("task");
        let hub = Arc::new(crate::domains::agent::runtime::stream::RuntimeStreamHub::default());
        let recorder = crate::domains::agent::runtime::stream::RuntimeRecorder::new(db.clone(), hub.clone());
        (db, recorder, hub)
    }

    fn job() -> DeviceJob {
        DeviceJob {
            id: "job-event".into(),
            workspace_id: "ws-job".into(),
            device_id: "device-1".into(),
            tool: "inspect".into(),
            args_json: serde_json::json!({}),
            state: DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some("task-1".into()),
            run_id: None,
            tool_call_id: Some("call-1".into()),
            driver_call_id: None,
            timeout_ms: 1_000,
            started_at: chrono::Utc::now(),
            ended_at: None,
        }
    }

    async fn workspace_db(root: &std::path::Path, db: &Db) -> Db {
        let workspace_db = Db::new(db.pool().clone()).with_workspace_store_root(root.join("data/agents"));
        workspace_db
            .create_workspace_store("ws-job")
            .await
            .expect("workspace store");
        workspace_db
    }

    /// Task 19C3 decision 3: `job:`-prefixed legacy ids are ordinary ids and
    /// both `job:` and `effect:` Jobs live in the workspace generation. No
    /// platform `device_jobs` or legacy runtime Job event is written.
    #[tokio::test]
    async fn job_prefixed_and_effect_ids_both_use_the_workspace_store() {
        let (db, _recorder, _hub) = fixture().await;
        let root = std::env::temp_dir().join(format!("tinyiothub-runtime-ports-{}", uuid::Uuid::new_v4()));
        let workspace_db = workspace_db(&root, &db).await;
        let store = WorkspaceDeviceJobStore::new(Arc::new(workspace_db.clone()));

        for (id, tool_call_id) in [("job:run-1:dev-1:1", "call-legacy"), ("effect:effect-1", "call-effect")] {
            let mut job = job();
            job.id = id.into();
            job.tool_call_id = Some(tool_call_id.into());
            store.create_job(&job).await.expect("workspace create");

            let wake = TaskWake {
                task_id: "task-1".into(),
                event_id: format!("device_job:{id}:done"),
                signal_json: serde_json::json!({"workspace_id":"ws-job","kind":"device_job_terminal"}).to_string(),
                dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
                created_at: chrono::Utc::now(),
                dispatched_at: None,
                resolved_at: None,
            };
            assert!(
                store
                    .finish_job(id, DeviceJobState::Done, &serde_json::json!({"ok": true}), Some(&wake))
                    .await
                    .expect("workspace finish")
            );
            assert!(
                workspace_db
                    .get_workspace_job("ws-job", id)
                    .await
                    .expect("workspace read")
                    .is_some(),
                "{id} must live in the workspace generation"
            );
            let platform_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM device_jobs WHERE id = ?")
                .bind(id)
                .fetch_one(db.pool())
                .await
                .expect("platform job count");
            assert_eq!(platform_jobs, 0, "no platform Job write for {id}");
            let wakes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id = ?")
                .bind(&wake.event_id)
                .fetch_one(
                    &workspace_db
                        .workspace_pool_for_testing("ws-job")
                        .await
                        .expect("workspace pool"),
                )
                .await
                .expect("workspace wake");
            assert_eq!(wakes, 1, "the TaskWake lives in the workspace outbox");
        }

        let platform_wakes: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM task_wake_outbox WHERE event_id LIKE 'device_job:%'")
                .fetch_one(db.pool())
                .await
                .expect("platform wake count");
        assert_eq!(platform_wakes, 0, "no platform wake is written");
        let events = db
            .runtime_events_after("ws-job", None, Some("task-1"), 100)
            .await
            .expect("runtime events");
        assert!(
            events.iter().all(|event| event.job_id.is_none()),
            "decision 2: workspace Jobs write no legacy runtime Job event"
        );
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    /// Per-method parity: the workspace store reproduces the legacy store's
    /// observable Job lifecycle on the same fixture (create/get/mark/progress/
    /// stop/active/finish/replay).
    #[tokio::test]
    async fn workspace_store_matches_legacy_store_semantics() {
        let (db, _recorder, _hub) = fixture().await;
        let root = std::env::temp_dir().join(format!("tinyiothub-runtime-parity-{}", uuid::Uuid::new_v4()));
        let workspace_db = workspace_db(&root, &db).await;
        let legacy = DbDeviceJobStore::new((*db).clone());
        let workspace = WorkspaceDeviceJobStore::new(Arc::new(workspace_db.clone()));

        let mut legacy_job = job();
        legacy_job.id = "job:run-1:dev-1:1".into();
        legacy_job.tool_call_id = Some("call-legacy".into());
        let mut workspace_job = legacy_job.clone();
        workspace_job.id = "job:run-2:dev-1:1".into();
        workspace_job.tool_call_id = Some("call-workspace".into());

        legacy.create_job(&legacy_job).await.expect("legacy create");
        workspace.create_job(&workspace_job).await.expect("workspace create");

        for (store, job) in [
            (&legacy as &dyn DeviceJobStore, &legacy_job),
            (&workspace as &dyn DeviceJobStore, &workspace_job),
        ] {
            assert_eq!(
                store.get_job("ws-job", &job.id).await.expect("get").expect("job").state,
                job.state,
                "{}: create/get parity",
                job.id
            );
            let running = store.mark_running(&job.id, "driver-1").await.expect("mark running");
            assert_eq!(running.state, DeviceJobState::Running);
            assert_eq!(running.driver_call_id.as_deref(), Some("driver-1"));
            assert!(matches!(
                store.mark_running(&job.id, "driver-2").await,
                Err(DeviceJobError::Conflict(_))
            ));

            assert!(
                store
                    .update_progress(&job.id, &serde_json::json!({"percent": 50}))
                    .await
                    .expect("progress")
            );
            assert!(
                !store
                    .update_progress(&job.id, &serde_json::json!({"percent": 50}))
                    .await
                    .expect("same revision")
            );

            let first = store.request_stop("ws-job", "task-1", &job.id).await.expect("stop");
            let second = store
                .request_stop("ws-job", "task-1", &job.id)
                .await
                .expect("stop replay");
            assert_eq!(first.stop_requested_at, second.stop_requested_at);
            assert!(matches!(
                store.request_stop("ws-job", "task-2", &job.id).await,
                Err(DeviceJobError::NotFound)
            ));

            let active = store.active_jobs(Some("ws-job"), false).await.expect("active");
            assert_eq!(active.len(), 1);
            assert_eq!(active[0].id, job.id);

            let wake = TaskWake {
                task_id: "task-1".into(),
                event_id: format!("device_job:{}:done", job.id),
                signal_json: serde_json::json!({"workspace_id":"ws-job"}).to_string(),
                dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
                created_at: chrono::Utc::now(),
                dispatched_at: None,
                resolved_at: None,
            };
            assert!(
                store
                    .finish_job(
                        &job.id,
                        DeviceJobState::Done,
                        &serde_json::json!({"ok": true}),
                        Some(&wake)
                    )
                    .await
                    .expect("finish")
            );
            assert!(
                !store
                    .finish_job(
                        &job.id,
                        DeviceJobState::Done,
                        &serde_json::json!({"ok": true}),
                        Some(&wake)
                    )
                    .await
                    .expect("finish replay")
            );
            let terminal = store.get_job("ws-job", &job.id).await.expect("get").expect("terminal");
            assert_eq!(terminal.state, DeviceJobState::Done);
            assert_eq!(terminal.result_json, Some(serde_json::json!({"ok": true})));
            assert!(terminal.ended_at.is_some());
            assert!(
                store
                    .active_jobs(Some("ws-job"), false)
                    .await
                    .expect("empty")
                    .is_empty()
            );
        }
        let _ = tokio::fs::remove_dir_all(&root).await;
    }

    #[tokio::test]
    async fn shared_evidence_adapter_publishes_the_exact_persisted_event() {
        let (db, recorder, hub) = fixture().await;
        let mut connection = hub
            .connect(db.clone(), "ws-job", "task-1", None)
            .await
            .expect("connect runtime stream");
        assert!(matches!(
            connection.next_frame().await,
            Some(crate::domains::agent::runtime::stream::RuntimeFrame::Snapshot { .. })
        ));
        let store = DbMediaEvidenceStore::new((*db).clone()).with_recorder(recorder);
        let now = chrono::Utc::now();
        store
            .insert_or_get(&tinyiothub_core::media::MediaEvidenceRecord {
                id: "evidence-live".into(),
                workspace_id: "ws-job".into(),
                task_id: Some("task-1".into()),
                run_id: None,
                tool_call_id: Some("call-1".into()),
                job_id: None,
                device_id: "device-1".into(),
                source_id: "camera".into(),
                source_seq: 4,
                media_time: now,
                mime: "image/jpeg".into(),
                size_bytes: 12,
                sha256: "abc".into(),
                storage_path: "private/evidence-live".into(),
                dedup_key: "evidence-live-dedup".into(),
                created_at: now,
                expires_at: now + chrono::Duration::days(1),
            })
            .await
            .expect("persist evidence");

        let persisted = db
            .runtime_event_by_id("ws-job", "evidence:evidence-live:created")
            .await
            .expect("query persisted event")
            .expect("persisted event");
        let Some(crate::domains::agent::runtime::stream::RuntimeFrame::Event { event, .. }) =
            connection.next_frame().await
        else {
            panic!("expected live evidence event");
        };
        assert_eq!(*event, persisted);
        assert_eq!(event.device_id.as_deref(), Some("device-1"));
        assert_eq!(event.source_id.as_deref(), Some("camera"));
        assert!(event.data.get("storagePath").is_none());
    }

    #[tokio::test]
    async fn shared_job_adapter_records_ordered_facts_with_stable_ids() {
        let (db, recorder, hub) = fixture().await;
        let mut connection = hub
            .connect(db.clone(), "ws-job", "task-1", None)
            .await
            .expect("connect runtime stream");
        assert!(matches!(
            connection.next_frame().await,
            Some(crate::domains::agent::runtime::stream::RuntimeFrame::Snapshot { .. })
        ));
        let store = DbDeviceJobStore::new((*db).clone()).with_recorder(recorder);
        store.create_job(&job()).await.expect("create");
        store.mark_running("job-event", "driver-1").await.expect("running");
        store
            .update_progress("job-event", &serde_json::json!({"percent":50}))
            .await
            .expect("progress");
        store
            .update_progress("job-event", &serde_json::json!({"percent":75}))
            .await
            .expect("next progress revision");
        assert!(
            !store
                .update_progress("job-event", &serde_json::json!({"percent":75}))
                .await
                .expect("same revision retry")
        );
        let first_stop = store.request_stop("ws-job", "task-1", "job-event").await.expect("stop");
        let second_stop = store
            .request_stop("ws-job", "task-1", "job-event")
            .await
            .expect("stop replay");
        assert_eq!(first_stop.stop_requested_at, second_stop.stop_requested_at);
        assert!(
            store
                .finish_job(
                    "job-event",
                    DeviceJobState::CancelledUnknown,
                    &serde_json::json!({"reason":"cancel_unknown"}),
                    None,
                )
                .await
                .expect("finish")
        );
        assert!(
            !store
                .finish_job(
                    "job-event",
                    DeviceJobState::CancelledUnknown,
                    &serde_json::json!({"reason":"cancel_unknown"}),
                    None,
                )
                .await
                .expect("finish replay")
        );
        let events = db
            .runtime_events_after("ws-job", None, Some("task-1"), 20)
            .await
            .expect("events")
            .into_iter()
            .filter(|event| event.job_id.is_some())
            .collect::<Vec<_>>();
        let ids: Vec<&str> = events.iter().map(|event| event.event_id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "job:job-event:accepted",
                "job:job-event:running",
                "job:job-event:progress:v1",
                "job:job-event:progress:v2",
                "job:job-event:stop_requested",
                "job:job-event:cancelled_unknown",
            ]
        );
        assert_eq!(events[2].data["revision"], 1);
        assert_eq!(events[2].data["progress"]["percent"], 50);
        assert_eq!(events[3].data["revision"], 2);
        assert_eq!(events[3].data["progress"]["percent"], 75);
        let mut streamed_ids = Vec::new();
        for _ in 0..events.len() {
            let Some(crate::domains::agent::runtime::stream::RuntimeFrame::Event { event, .. }) =
                connection.next_frame().await
            else {
                panic!("expected every persisted revision on the live stream");
            };
            streamed_ids.push(event.event_id.to_string());
        }
        assert_eq!(streamed_ids, ids);
        assert!(events.iter().all(|event| event.task_id.as_deref() == Some("task-1")));
        assert!(events.iter().all(|event| event.job_id.as_deref() == Some("job-event")));
        assert!(
            events
                .iter()
                .all(|event| event.device_id.as_deref() == Some("device-1"))
        );
    }

    #[tokio::test]
    async fn watch_adapter_records_create_and_completion_for_owner_task() {
        let (db, recorder, _hub) = fixture().await;
        let store = WatchRuleStoreAdapter::new((*db).clone()).with_recorder(recorder);
        let rule = tinyiothub_core::watch::WatchRule {
            id: "watch-event".into(),
            workspace_id: "ws-job".into(),
            condition: tinyiothub_core::watch::WatchCondition::Availability {
                thing_id: "device-1".into(),
                online: true,
            },
            persistent: false,
            timeout_secs: 60,
            created_at: chrono::Utc::now().to_rfc3339(),
            owner_task_id: Some("task-1".into()),
        };
        store.insert(&rule).await.expect("insert watch");
        let wake = tinyiothub_core::agent_task::TaskWake {
            task_id: "task-1".into(),
            event_id: "watch-complete".into(),
            signal_json: serde_json::json!({"workspace_id":"ws-job"}).to_string(),
            dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
            created_at: chrono::Utc::now(),
            dispatched_at: None,
            resolved_at: None,
        };
        assert!(
            store
                .complete_task_watch("ws-job", "watch-event", &wake)
                .await
                .expect("complete")
        );
        let events = db
            .runtime_events_after("ws-job", None, Some("task-1"), 20)
            .await
            .expect("events")
            .into_iter()
            .filter(|event| event.event_type.starts_with("watch."))
            .collect::<Vec<_>>();
        let ids: Vec<&str> = events.iter().map(|event| event.event_id.as_str()).collect();
        assert_eq!(ids, ["watch:watch-event:created", "watch:watch-event:completed"]);
        assert_eq!(events[0].event_type, "watch.created");
        assert_eq!(events[1].event_type, "watch.completed");
        assert!(events.iter().all(|event| event.task_id.as_deref() == Some("task-1")));
    }
    #[tokio::test]
    async fn watch_adapter_publishes_and_replays_cancellation_and_generic_deletion() {
        let (db, recorder, hub) = fixture().await;
        let store = WatchRuleStoreAdapter::new((*db).clone()).with_recorder(recorder);
        let mut connection = hub
            .connect(db.clone(), "ws-job", "task-1", None)
            .await
            .expect("connect runtime stream");
        assert!(matches!(
            connection.next_frame().await,
            Some(crate::domains::agent::runtime::stream::RuntimeFrame::Snapshot { .. })
        ));
        let rule = |id: &str| tinyiothub_core::watch::WatchRule {
            id: id.into(),
            workspace_id: "ws-job".into(),
            condition: tinyiothub_core::watch::WatchCondition::Availability {
                thing_id: "device-1".into(),
                online: true,
            },
            persistent: false,
            timeout_secs: 60,
            created_at: chrono::Utc::now().to_rfc3339(),
            owner_task_id: Some("task-1".into()),
        };
        store.insert(&rule("watch-cancel-1")).await.expect("first watch");
        store.insert(&rule("watch-cancel-2")).await.expect("second watch");
        for id in ["watch-cancel-1", "watch-cancel-2"] {
            let Some(crate::domains::agent::runtime::stream::RuntimeFrame::Event { event, .. }) =
                connection.next_frame().await
            else {
                panic!("expected live creation");
            };
            assert_eq!(event.event_id, format!("watch:{id}:created"));
        }
        assert_eq!(
            store
                .delete_task_watches("ws-job", "task-1")
                .await
                .expect("cancel watches"),
            2
        );
        for id in ["watch-cancel-1", "watch-cancel-2"] {
            let Some(crate::domains::agent::runtime::stream::RuntimeFrame::Event { event, .. }) =
                connection.next_frame().await
            else {
                panic!("expected live cancellation");
            };
            assert_eq!(event.event_id, format!("watch:{id}:deleted"));
            assert_eq!(event.event_type, "watch.deleted");
        }
        store.insert(&rule("watch-generic")).await.expect("generic watch");
        assert!(matches!(
            connection.next_frame().await,
            Some(crate::domains::agent::runtime::stream::RuntimeFrame::Event { .. })
        ));
        assert!(store.delete("ws-job", "watch-generic").await.expect("generic delete"));
        let Some(crate::domains::agent::runtime::stream::RuntimeFrame::Event { event, .. }) =
            connection.next_frame().await
        else {
            panic!("expected live generic deletion");
        };
        assert_eq!(event.event_id, "watch:watch-generic:deleted");
        assert!(
            !store
                .delete("ws-job", "watch-generic")
                .await
                .expect("idempotent delete")
        );
        assert_eq!(
            store
                .delete_task_watches("ws-job", "task-1")
                .await
                .expect("empty cancel"),
            0
        );
        let replayed = db
            .runtime_events_after("ws-job", None, Some("task-1"), 20)
            .await
            .expect("persisted replay")
            .into_iter()
            .filter(|event| event.event_type == "watch.deleted")
            .map(|event| event.event_id)
            .collect::<Vec<_>>();
        assert_eq!(
            replayed,
            [
                "watch:watch-cancel-1:deleted",
                "watch:watch-cancel-2:deleted",
                "watch:watch-generic:deleted",
            ]
        );
    }

    #[tokio::test]
    async fn approved_job_submission_replays_the_existing_tool_call() {
        let (db, _, _) = fixture().await;
        let store = DbDeviceJobStore::new(db.as_ref().clone());
        let job = DeviceJob {
            id: "approved-job".into(),
            workspace_id: "ws-job".into(),
            device_id: "device-1".into(),
            tool: "set_speed".into(),
            args_json: serde_json::json!({"speed": 1}),
            state: tinyiothub_core::device_job::DeviceJobState::Accepted,
            stop_requested_at: None,
            progress_json: None,
            result_json: None,
            task_id: Some("task-1".into()),
            run_id: Some("run-1".into()),
            tool_call_id: Some("call-approved".into()),
            driver_call_id: None,
            timeout_ms: 5_000,
            started_at: chrono::Utc::now(),
            ended_at: None,
        };
        let (first, inserted) = store.submit_approved_job(&job).await.expect("first submit");
        assert!(inserted);
        let (second, inserted) = store.submit_approved_job(&job).await.expect("replay");
        assert!(!inserted, "a retried approved submission must not insert twice");
        assert_eq!(second.map(|job| job.id), Some("approved-job".into()));
        assert_eq!(first.map(|job| job.id), Some("approved-job".into()));
    }
}

#[cfg(test)]
mod approval_timeout_tests {
    use super::*;

    async fn fixture() -> (tinyiothub_storage::Db, ApprovalTimeoutAdapter) {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        tinyiothub_storage::seed::seed_system(&tinyiothub_storage::Db::new(pool.clone()))
            .await
            .unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')").execute(&pool).await.unwrap();
        let db = tinyiothub_storage::Db::new(pool);
        let adapter = ApprovalTimeoutAdapter {
            db: db.clone(),
            sse: Arc::new(crate::domains::event::sse_manager::SseConnectionManager::new()),
            alarm_service: Arc::new(crate::domains::alarm::service::AlarmService::new(Arc::new(db.clone()))),
            notify: None,
        };
        (db, adapter)
    }

    #[tokio::test]
    async fn stale_approval_escalates_to_ticket() {
        let (db, adapter) = fixture().await;

        // 一条 awaiting_approval 且 judged_at 在 25h 前
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET judged_at = datetime('now', '-25 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        // 一条新的（不应命中）
        let fresh = db
            .insert_judgment("ws1", None, None, Some("t2"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &fresh,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "新的",
            "{}",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_approvals(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);

        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some());
        let f = db.find_judgment_by_id(&fresh, "ws1").await.unwrap().unwrap();
        assert_eq!(
            f.status,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            "fresh untouched"
        );
    }

    /// E2：investigating 超 SLA → dispatch_suppressed 标记（不开票不计预算）。
    #[tokio::test]
    async fn stale_investigating_marked_suppressed_no_ticket() {
        let (db, adapter) = fixture().await;
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-40 minutes') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::mark_stale_investigating(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap();
        assert_eq!(
            j.unwrap().status,
            tinyiothub_storage::judgment::JudgmentStatus::DispatchSuppressed
        );
        // 不开票
        assert_eq!(
            db.count_by_statuses("ws1", &[tinyiothub_storage::judgment::JudgmentStatus::Escalated])
                .await
                .unwrap(),
            0
        );
    }

    /// X5：dismissed 超 72h 且关联报警仍 Active → 重浮 investigating（否决有兜底）。
    #[tokio::test]
    async fn dismissed_with_active_alarm_resurfaces() {
        let (db, adapter) = fixture().await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let alarm = tinyiothub_storage::alarm::Alarm::new(
            "t1".to_string(),
            None,
            None,
            tinyiothub_storage::alarm::AlarmType::PropertyThreshold,
            tinyiothub_storage::alarm::AlarmLevel::Warning,
            "温度越限".to_string(),
            None,
            None,
            Some("ws1".to_string()),
        );
        let alarm_id = alarm.id.clone();
        db.insert_alarm(&alarm).await.unwrap();

        let jid = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
            None,
        )
        .await
        .unwrap();
        let flipped = db
            .transit_judgment(
                &jid,
                tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
                tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
                None,
            )
            .await
            .unwrap();
        assert!(flipped, "awaiting_approval → dismissed 是合法边");
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-73 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::resurface_stale_dismissed(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1, "报警仍 Active 的 dismissed 应重浮");
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Investigating);
    }

    /// X5 反向：报警已消（Resolved）的 dismissed 保持终态——用户对了一半。
    #[tokio::test]
    async fn dismissed_with_resolved_alarm_stays() {
        let (db, adapter) = fixture().await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let alarm = tinyiothub_storage::alarm::Alarm::new(
            "t1".to_string(),
            None,
            None,
            tinyiothub_storage::alarm::AlarmType::PropertyThreshold,
            tinyiothub_storage::alarm::AlarmLevel::Warning,
            "温度越限".to_string(),
            None,
            None,
            Some("ws1".to_string()),
        );
        let alarm_id = alarm.id.clone();
        db.insert_alarm(&alarm).await.unwrap();
        sqlx::query("UPDATE thing_alarms SET is_resolved = 1 WHERE id = ?")
            .bind(&alarm_id)
            .execute(db.pool())
            .await
            .unwrap();

        let jid = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
            None,
        )
        .await
        .unwrap();
        db.transit_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-73 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::resurface_stale_dismissed(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 0, "报警已消的 dismissed 不重浮");
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Dismissed);
    }

    /// R-F1 守卫：同报警已有新的未终态判断 → 重浮跳过（旧行保持 dismissed，
    /// 不撞 judgments_active_alarm 唯一索引；新判断才是活的）。
    #[tokio::test]
    async fn dismissed_with_newer_open_judgment_not_resurfaced() {
        let (db, adapter) = fixture().await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let alarm = tinyiothub_storage::alarm::Alarm::new(
            "t1".to_string(),
            None,
            None,
            tinyiothub_storage::alarm::AlarmType::PropertyThreshold,
            tinyiothub_storage::alarm::AlarmLevel::Warning,
            "温度越限".to_string(),
            None,
            None,
            Some("ws1".to_string()),
        );
        let alarm_id = alarm.id.clone();
        db.insert_alarm(&alarm).await.unwrap();

        // j1：dismissed（旧，72h+）
        let j1 = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &j1,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
            None,
        )
        .await
        .unwrap();
        db.transit_judgment(
            &j1,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-73 hours') WHERE id = ?")
            .bind(&j1)
            .execute(db.pool())
            .await
            .unwrap();
        // j2：同报警的新 investigating 判断（dismissed 不占 active 索引,合法建行）
        let j2 = db
            .insert_judgment("ws1", Some(&alarm_id), None, Some("t1"), "annotate")
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(72)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::resurface_stale_dismissed(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 0, "新判断接管的重浮必须跳过");
        let old = db.find_judgment_by_id(&j1, "ws1").await.unwrap().unwrap();
        assert_eq!(
            old.status,
            tinyiothub_storage::judgment::JudgmentStatus::Dismissed,
            "旧行保持 dismissed（不撞唯一索引）"
        );
        let new = db.find_judgment_by_id(&j2, "ws1").await.unwrap().unwrap();
        assert_eq!(
            new.status,
            tinyiothub_storage::judgment::JudgmentStatus::Investigating,
            "新判断不受影响"
        );
    }

    /// G2：SLA 升级时 notify=Some → 「需要你」通知真实派发且投递目标正确（X1 适配器分支）。
    #[tokio::test]
    async fn sla_escalation_sends_needs_you_notify() {
        struct RecordingChannel {
            sent: std::sync::Arc<std::sync::Mutex<Vec<(String, Vec<String>)>>>,
        }
        #[async_trait::async_trait]
        impl crate::domains::notify::dto::NotificationChannel for RecordingChannel {
            fn channel_type(&self) -> tinyiothub_core::notification_types::NotificationChannelType {
                tinyiothub_core::notification_types::NotificationChannelType::Sse
            }
            async fn send(
                &self,
                message: &crate::domains::notify::dto::NotificationMessage,
            ) -> std::result::Result<(), String> {
                self.sent
                    .lock()
                    .unwrap()
                    .push((message.title.clone(), message.recipients.clone()));
                Ok(())
            }
            async fn is_available(&self) -> bool {
                true
            }
            fn get_config(&self) -> std::collections::HashMap<String, String> {
                Default::default()
            }
        }

        let (db, _) = fixture().await;
        let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut manager = crate::domains::notify::NotificationManager::new(std::sync::Arc::new(db.clone()));
        manager.register_channel(Box::new(RecordingChannel { sent: sent.clone() }));
        let adapter = ApprovalTimeoutAdapter {
            db: db.clone(),
            sse: std::sync::Arc::new(crate::domains::event::sse_manager::SseConnectionManager::new()),
            alarm_service: std::sync::Arc::new(crate::domains::alarm::service::AlarmService::new(std::sync::Arc::new(
                db.clone(),
            ))),
            notify: Some(std::sync::Arc::new(manager)),
        };

        // 一条 awaiting_approval 且 judged_at 在 25h 前 → SLA 升级
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET judged_at = datetime('now', '-25 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_approvals(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);

        // spawn 派发不 await——短轮询等异步落盘
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while sent.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let got = sent.lock().unwrap();
        assert_eq!(got.len(), 1, "SLA 升级应发「需要你」通知");
        // 外部声音 #5：断言投递目标——「例外到人」的路由维度，不只是派发次数
        assert_eq!(got[0].1, vec!["ws1".to_string()], "通知必须投递到本工作区");
    }

    /// 对账恢复（2026-09-16 实测：run 完成且 verdict 合法，但 RunRecorded
    /// 事件丢失 → judgment 永久卡 investigating）：超 SLA 的 investigating
    /// 判断若已有完成的 run，从存储的 RunReport 重投影路由，而不是标
    /// dispatch_suppressed。
    #[tokio::test]
    async fn stale_investigating_reprojects_from_completed_run() {
        let (db, adapter) = fixture().await;
        // FK 链：thing → rule → alarm → judgment
        sqlx::query("INSERT INTO things (id, name, workspace_id, created_at, updated_at) VALUES ('t1','t1','ws1','2025-01-01','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        sqlx::query("INSERT INTO thing_alarm_rules (id, thing_id, rule_name, rule_type, condition_config, alarm_level, workspace_id) VALUES ('r1','t1','温度阈值','threshold','{}','warning','ws1')")
            .execute(db.pool()).await.unwrap();
        sqlx::query("INSERT INTO thing_alarms (id, thing_id, rule_id, workspace_id, alarm_level, alarm_message, alarm_time) VALUES ('a1','t1','r1','ws1','warning','温度越限','2025-01-01')")
            .execute(db.pool()).await.unwrap();
        let jid = db
            .insert_judgment("ws1", Some("a1"), None, Some("t1"), "annotate")
            .await
            .unwrap();
        // 卡在 investigating 超过 SLA
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-40 minutes') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        // 完成的调查 run（事件丢失前的真实产出）：verdict = noise
        let report = tinyiothub_core::agent_runs::RunReport {
            run_id: "run-lost-1".to_string(),
            workspace_id: "ws1".to_string(),
            trigger: "alarm".to_string(),
            outcome: tinyiothub_core::agent_runs::Outcome::NoActionNeeded,
            summary: "分析了设备状态，温度属正常波动。\n```json\n{\"verdict\": \"noise\", \"reason\": \"正常波动\", \"suggested_action\": null, \"action_category\": \"other\"}\n```".to_string(),
            actions: vec![],
            verified: false,
            duration_ms: 1000,
            tool_calls: 1,
            tokens: 100,
            end_reason: None,
            thing_id: Some("t1".to_string()),
            trace: vec![],
            task_count: None,
            snapshot: None,
            task_id: None,
            wake_event_id: None,
        };
        db.insert_agent_run(&report, Some("alarm:t1:r1"), None).await.unwrap();

        let cutoff = (chrono::Utc::now() - chrono::Duration::minutes(30)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::mark_stale_investigating(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 0, "有完成 run 的滞留判断应被重投影，而不是标 suppressed");
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(
            j.status,
            tinyiothub_storage::judgment::JudgmentStatus::NoiseArchived,
            "应从 run 的 verdict 恢复路由"
        );
        assert_eq!(j.run_id.as_deref(), Some("run-lost-1"), "run_id 应回填");
    }

    /// E2/T-16：executing 超 SLA → escalated + 人工确认工单。
    #[tokio::test]
    async fn stale_executing_escalates_to_human_confirm_ticket() {
        let (db, adapter) = fixture().await;
        let jid = db
            .insert_judgment("ws1", None, None, Some("t1"), "annotate")
            .await
            .unwrap();
        db.judge_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentVerdict::SelfHealable,
            "可重连",
            "{}",
            Some("重连"),
            Some("connection_recovery"),
            None,
            None,
        )
        .await
        .unwrap();
        db.transit_judgment(
            &jid,
            tinyiothub_storage::judgment::JudgmentStatus::AwaitingApproval,
            tinyiothub_storage::judgment::JudgmentStatus::Executing,
            None,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE judgments SET state_entered_at = datetime('now', '-2 hours') WHERE id = ?")
            .bind(&jid)
            .execute(db.pool())
            .await
            .unwrap();
        let cutoff = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let n = tinyiothub_runtime::ports::ApprovalTimeoutStore::escalate_stale_executing(&adapter, &cutoff)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let j = db.find_judgment_by_id(&jid, "ws1").await.unwrap().unwrap();
        assert_eq!(j.status, tinyiothub_storage::judgment::JudgmentStatus::Escalated);
        assert!(j.ticket_id.is_some(), "人工确认工单已建");
    }
}

/// E1：HubLedger / WatchRuleStore 的 db 适配器测试（TDD Step 1——trait 与
/// 适配器尚不存在，本模块必须先编译失败）。
#[cfg(test)]
mod watch_ports_tests {
    use super::*;
    use tinyiothub_core::watch::{CompareOp, WatchCondition, WatchRule};
    use tinyiothub_runtime::ports::{HubLedger, WatchRuleStore};

    async fn fixture_db() -> tinyiothub_storage::Db {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        tinyiothub_storage::Db::new(pool)
    }

    fn sample_rule(id: &str, ws: &str) -> WatchRule {
        WatchRule {
            id: id.into(),
            workspace_id: ws.into(),
            condition: WatchCondition::Property {
                thing_id: "t1".into(),
                property: "温度".into(),
                op: CompareOp::Gt,
                value: serde_json::json!(30.0),
            },
            persistent: true,
            timeout_secs: 600,
            created_at: "2026-10-20T00:00:00Z".into(),
            owner_task_id: None,
        }
    }

    /// F4 兜底：append 按 (workspace, 日) 计数，日增越阈 warn 一次
    /// （metric="hub_ledger_volume"），同日后续落账不重复刷；他区/他日独立。
    #[tokio::test(flavor = "current_thread")]
    async fn ledger_volume_warns_once_when_daily_threshold_crossed() {
        use tracing_subscriber::fmt::MakeWriter;

        /// 内存 writer：捕获 fmt 输出断言 warn 次数与 metric 字段。
        #[derive(Clone, Default)]
        struct LogBuf(Arc<std::sync::Mutex<Vec<u8>>>);

        impl std::io::Write for LogBuf {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        impl<'a> MakeWriter<'a> for LogBuf {
            type Writer = LogBuf;
            fn make_writer(&self) -> Self::Writer {
                self.clone()
            }
        }

        let buf = LogBuf::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buf.clone())
            .with_ansi(false)
            .finish();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

        // set_default 守卫：current_thread runtime 全程同线程轮询，
        // 线程本地默认订阅者覆盖整个异步块（with_default 只包闭包调用期，
        // 不包返回 future 的轮询期，捕不到）。
        let _guard = tracing::subscriber::set_default(subscriber);
        let db = fixture_db().await;
        let adapter = HubLedgerAdapter::new(db);
        // 预置计数到阈值前一行（不灌 10 万行——计数逻辑即被测对象）。
        adapter
            .daily_counts
            .insert(("ws1".to_string(), today.clone()), HUB_LEDGER_DAILY_WARN_THRESHOLD - 1);

        HubLedger::append(&adapter, "ws1", "watch_hit", Some("t1"), serde_json::json!({"v": 1}))
            .await
            .unwrap(); // 越阈 → warn
        HubLedger::append(&adapter, "ws1", "watch_hit", Some("t1"), serde_json::json!({"v": 1}))
            .await
            .unwrap(); // 同日再越 → 不重复 warn
        HubLedger::append(&adapter, "ws2", "watch_hit", None, serde_json::json!({"v": 1}))
            .await
            .unwrap(); // 他区计数独立 → 不 warn

        let logs = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        let hits = logs.matches("hub_ledger_volume").count();
        assert_eq!(hits, 1, "越阈当日应 warn 恰好一次: {logs}");
        assert!(logs.contains("hub_ledger 日增行数越阈"), "{logs}");
    }

    /// append 后能从 db 读回（kind/thing_id/payload 落列，ts 由适配器生成）。
    #[tokio::test]
    async fn ledger_append_then_read_back() {
        let db = fixture_db().await;
        let adapter = HubLedgerAdapter::new(db.clone());
        HubLedger::append(
            &adapter,
            "ws1",
            "watch_hit",
            Some("t1"),
            serde_json::json!({"v": 1, "rule_id": "r1"}),
        )
        .await
        .unwrap();

        let row: (String, String, Option<String>, String) =
            sqlx::query_as("SELECT workspace_id, kind, thing_id, payload FROM hub_ledger")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(row.0, "ws1");
        assert_eq!(row.1, "watch_hit");
        assert_eq!(row.2.as_deref(), Some("t1"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&row.3).unwrap(),
            serde_json::json!({"v": 1, "rule_id": "r1"})
        );
    }

    /// insert 后 list_all 可见，condition 完整 JSON 往返、kind 列存 type tag。
    #[tokio::test]
    async fn rule_insert_then_list_all_visible() {
        let db = fixture_db().await;
        let adapter = WatchRuleStoreAdapter::new(db.clone());
        let rule = sample_rule("r1", "ws1");
        WatchRuleStore::insert(&adapter, &rule).await.unwrap();

        // kind 列存 condition 的 serde type tag
        let (kind,): (String,) = sqlx::query_as("SELECT kind FROM watch_rules WHERE id = 'r1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(kind, "property");

        let all = WatchRuleStore::list_all(&adapter).await.unwrap();
        assert_eq!(all.len(), 1);
        let got = &all[0];
        assert_eq!(got.id, "r1");
        assert_eq!(got.workspace_id, "ws1");
        assert_eq!(got.condition, rule.condition, "condition JSON 往返不失真");
        assert!(got.persistent);
        assert_eq!(got.timeout_secs, 600);
        assert_eq!(got.created_at, "2026-10-20T00:00:00Z");
    }

    #[tokio::test]
    async fn rebuild_lists_only_watches_whose_owner_task_is_nonterminal() {
        let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        let db = tinyiothub_storage::Db::new(pool);
        tinyiothub_storage::seed::seed_system(&db).await.unwrap();
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES ('ws1','ws1','tenant-default-001','2025-01-01','2025-01-01')")
            .execute(db.pool())
            .await
            .unwrap();
        for (task_id, status) in [
            ("task-active", tinyiothub_core::agent_task::TaskStatus::Active),
            ("task-paused", tinyiothub_core::agent_task::TaskStatus::Paused),
            ("task-terminal", tinyiothub_core::agent_task::TaskStatus::Completed),
        ] {
            let now = chrono::Utc::now();
            let task = tinyiothub_core::agent_task::AgentTask {
                id: task_id.into(),
                workspace_id: "ws1".into(),
                idempotency_key: task_id.into(),
                objective: "watch pump".into(),
                session_key: Some(format!("agent:ws1:default/{task_id}")),
                created_by: "user-1".into(),
                status: tinyiothub_core::agent_task::TaskStatus::Active,
                current_run_id: None,
                state_version: 0,
                created_at: now,
                updated_at: now,
                ended_at: None,
            };
            let wake = tinyiothub_core::agent_task::TaskWake {
                task_id: task_id.into(),
                event_id: format!("create:{task_id}"),
                signal_json: serde_json::json!({"workspace_id": "ws1"}).to_string(),
                dispatch_state: tinyiothub_core::agent_task::WakeDispatchState::Pending,
                created_at: now,
                dispatched_at: None,
                resolved_at: None,
            };
            db.create_task_with_wake(&task, &wake).await.unwrap();
            if status != tinyiothub_core::agent_task::TaskStatus::Active {
                db.transition_task("ws1", task_id, 0, status, None).await.unwrap();
            }
            let mut rule = sample_rule(&format!("rule-{task_id}"), "ws1");
            rule.owner_task_id = Some(task_id.into());
            let condition = serde_json::to_value(&rule.condition).unwrap();
            tinyiothub_storage::watch::insert_rule(
                db.pool(),
                &tinyiothub_storage::watch::WatchRuleRow {
                    id: rule.id,
                    workspace_id: rule.workspace_id,
                    kind: "property".into(),
                    condition: condition.to_string(),
                    persistent: 1,
                    timeout_secs: 600,
                    created_at: rule.created_at,
                    owner_task_id: rule.owner_task_id,
                },
            )
            .await
            .unwrap();
        }

        let adapter = WatchRuleStoreAdapter::new(db);
        let restored = WatchRuleStore::list_all(&adapter).await.unwrap();
        let ids: Vec<&str> = restored.iter().map(|rule| rule.id.as_str()).collect();
        assert_eq!(ids, ["rule-task-active", "rule-task-paused"]);
    }

    /// delete 跨工作区落空（多租户隔离），本工作区删得掉。
    #[tokio::test]
    async fn rule_delete_cross_workspace_misses() {
        let db = fixture_db().await;
        let adapter = WatchRuleStoreAdapter::new(db.clone());
        WatchRuleStore::insert(&adapter, &sample_rule("r1", "ws1"))
            .await
            .unwrap();

        assert!(
            !WatchRuleStore::delete(&adapter, "ws2", "r1").await.unwrap(),
            "跨工作区删除必须落空"
        );
        assert_eq!(WatchRuleStore::list_all(&adapter).await.unwrap().len(), 1);
        assert!(WatchRuleStore::delete(&adapter, "ws1", "r1").await.unwrap());
        assert!(WatchRuleStore::list_all(&adapter).await.unwrap().is_empty());
    }
}
