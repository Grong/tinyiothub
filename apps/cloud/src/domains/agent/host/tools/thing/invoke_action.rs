// 6. invoke_action — execute a device action (type='device' guard)

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_agent::runtime::thing_agent::seams::approval::{
    ApprovalDecision, ApprovalInput, ApprovalSeam, DefaultApproval,
};
use tinyiothub_agent::runtime::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL};
use tinyiothub_runtime::access::resources::ResourceResolver;
use tinyiothub_skills::trust::ToolSafety;

use super::{PendingActionStore, resource_specs, store_pending_action, tool_err, tool_ok, validate_action_params};
use crate::domains::thing::service::ThingService;

pub struct InvokeActionTool {
    // pub(crate) so the T11 autonomous variant (tools/autonomous_invoke.rs,
    // O18 thin wrapper) can construct it — no logic change.
    pub(crate) thing_service: Arc<ThingService>,
    pub(crate) pool: SqlitePool,
    pub(crate) workspace_id: String,
    pub(crate) data_server: Option<Arc<tinyiothub_runtime::DataServer>>,
    pub(crate) pending_actions: Arc<PendingActionStore>,
    /// E1 仲裁（Task 9）：每设备互斥锁注册表，跨 chat/自治 run 共享；
    /// None = 不仲裁（既有行为不变）。
    pub(crate) arbitration: Option<Arc<ArbitrationRegistry>>,
    /// busy 拒绝里的持有者名（人读得懂的拒绝）——chat 会话/值守 run 标识。
    pub(crate) arbitration_holder: String,
    /// E2 Approval 接缝（None = 内置 DefaultApproval）。
    pub(crate) approval: Option<Arc<dyn ApprovalSeam>>,
    /// E4a 声明校验（None = 未接线，既有行为不变）。接线后 fail-closed：
    /// 未声明操作/参数越界直接拒绝，不回退 DB schema。
    pub access: Option<Arc<dyn tinyiothub_runtime::access::validator::AccessCallValidator>>,
    /// E4c 声明资源解析（None = 未接线）：接线后按工具声明 claim 资源，
    /// 未声明 resources 的工具仍 claim 合成 `__device__`（E1 语义）。
    pub resources: Option<Arc<dyn ResourceResolver>>,
}

impl Attributable for InvokeActionTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for InvokeActionTool {
    fn name(&self) -> &str {
        "invoke_action"
    }

    fn description(&self) -> &str {
        "对物执行操作。仅 thingType='device' 的物支持此操作。\
         如果工作空间启用了 require_action_confirm，则需要确认才能执行。\
         当你需要控制设备（如开关、重启、设置参数）时使用此工具。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": {
                    "type": "string",
                    "description": "物ID（必需）"
                },
                "actionName": {
                    "type": "string",
                    "description": "操作名称（必需）"
                },
                "params": {
                    "type": "object",
                    "description": "操作参数（可选，JSON 键值对）"
                }
            },
            "required": ["thingId", "actionName"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            // alias：提案/报告的 parameters 常随模板写成蛇形键
            // （thing_id），与 schema 的驼峰并容。
            #[serde(alias = "thing_id")]
            thing_id: String,
            #[serde(alias = "action_name", alias = "action")]
            action_name: String,
            params: Option<Value>,
        }

        let raw_args = args.clone();
        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {}", e))?;

        // 1. Verify thing exists and check type
        let thing = self
            .thing_service
            .get_thing(&input.thing_id, &self.workspace_id)
            .await
            .map_err(|e| anyhow::anyhow!("物不存在: {}", e))?;

        if thing.thing_type != "device" {
            return tool_err(format!(
                "操作不支持: 物类型为 '{}'，仅 'device' 类型物支持 invoke_action。\
                 对于 space/line/building 类型，请使用其属性或知识库。",
                thing.thing_type
            ));
        }

        // 1b. E4a declaration validation（fail-closed when wired）：未声明
        // 操作、参数 schema/范围不符、声明缺失都在此拒绝；仲裁/确认门不动。
        if let Some(access) = &self.access
            && let Err(e) = access
                .validate(
                    &self.workspace_id,
                    &input.thing_id,
                    &input.action_name,
                    input.params.as_ref(),
                )
                .await
        {
            // Declaration rejections are user-actionable → failed ToolResult
            // (the model can correct/retry); only true infrastructure errors
            // surface as Err.
            if e.is_user_actionable() {
                return tool_err(e.to_string());
            }
            return Err(anyhow::anyhow!(e.to_string()));
        }

        // 2. Check require_action_confirm from workspace
        let require_confirm: bool = tinyiothub_storage::Db::new(self.pool.clone())
                .get_workspace_require_action_confirm(&self.workspace_id)
                .await
                .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?
                // Fail CLOSED (eng-review T7): a missing workspace row means
                // something is wrong — require confirmation (design default ON)
                .unwrap_or(1i32)
            != 0;

        // 3. Check if action exists in device_commands table
        let command_exists: bool = tinyiothub_storage::Db::new(self.pool.clone())
            .count_thing_action_by_name(&input.thing_id, &input.action_name)
            .await
            .map(|c| c > 0)
            .unwrap_or(false);

        if !command_exists {
            return tool_err(format!(
                "操作 '{}' 未在物 {} 上注册。请检查可用的操作列表。",
                input.action_name, input.thing_id
            ));
        }

        // 3b. Validate params against the action's parameter schema (design 三;
        // eng-review T7)
        let params_schema: Option<String> = tinyiothub_storage::Db::new(self.pool.clone())
            .find_thing_action_parameters(&input.thing_id, &input.action_name)
            .await
            .map_err(|e| anyhow::anyhow!("数据库查询失败: {}", e))?;
        if let Some(ref schema_json) = params_schema
            && let Err(msg) = validate_action_params(schema_json, input.params.as_ref())
        {
            return tool_err(msg);
        }

        // 4. Execute or request confirmation（E2：经 Approval 接缝裁决；
        // 默认实现与 E2 前语义逐项一致，Pending 分支 payload 不变）。
        let approval: Arc<dyn ApprovalSeam> = self.approval.clone().unwrap_or_else(|| Arc::new(DefaultApproval));
        match approval.decide(&ApprovalInput {
            workspace_id: &self.workspace_id,
            tool_name: self.name(),
            args: &raw_args,
            thing_id: Some(&input.thing_id),
            trigger: "chat",
            require_confirm,
            // invoke_action 在工厂里声明为 Destructive（信任门依赖此分级）。
            safety: ToolSafety::Destructive,
        }) {
            ApprovalDecision::Pending { .. } => {
                let token = store_pending_action(
                    &self.pending_actions,
                    input.thing_id.clone(),
                    input.action_name.clone(),
                    input.params.clone(),
                    self.workspace_id.clone(),
                );
                return tool_ok(json!({
                    "thingId": input.thing_id,
                    "actionName": input.action_name,
                    "status": "confirmation_required",
                    "token": token,
                    "message": "该操作需要用户确认后才能执行。请使用确认接口提交 token。",
                    "requireConfirm": true
                }));
            }
            ApprovalDecision::Decided(reason) if reason.executes() => {}
            ApprovalDecision::Decided(reason) => {
                return tool_err(format!("approval {}: action NOT executed", reason.code()));
            }
        }

        // 5. E1/E4c 仲裁：执行前按声明资源 claim——TTL（默认 10min）内
        // 已被另一持有者（chat 会话/值守 run）占用时拒绝，busy 文案带持有者
        // 名与资源 id；持有者崩溃不死锁（TTL 过期自动可再 acquire）。
        // 未接线资源解析时沿用 E1 合成 `__device__`（每设备互斥）。
        // Lease RAII：本次调用返回即释放。
        // 确认门路径：mint token 不持锁，但批准后的真实下发经
        // dispatch_command 同样 acquire（autonomous_invoke.rs，Task 9 fix）。
        let _lease = match &self.arbitration {
            Some(registry) => {
                let acquired = match &self.resources {
                    Some(resolver) => match resolver
                        .resolve_action(&self.workspace_id, &input.thing_id, &input.action_name)
                        .await
                    {
                        Ok(claims) => registry.try_acquire_resources(
                            &input.thing_id,
                            self.arbitration_holder.clone(),
                            &resource_specs(&claims),
                            DEFAULT_LEASE_TTL,
                        ),
                        Err(e) => return tool_err(format!("资源解析失败: {e}")),
                    },
                    None => registry.try_acquire(&input.thing_id, self.arbitration_holder.clone(), DEFAULT_LEASE_TTL),
                };
                match acquired {
                    Ok(lease) => Some(lease),
                    // 声明资源路径带 holder + resource（spec §2.4）；兼容路径
                    // 保持 E1 既有文案。
                    Err(busy) if self.resources.is_some() => return tool_err(busy.to_string()),
                    Err(busy) => {
                        return tool_err(format!("busy: 设备正被 {} 占用，请稍后重试", busy.holder));
                    }
                }
            }
            None => None,
        };

        // Execute via DataServer if available.
        // NOTE: this dispatch tail (through the closing brace of this match)
        // is mirrored by `dispatch_command` in autonomous_invoke.rs — if you
        // change it here, keep the mirror in sync.
        match self.data_server.clone() {
            Some(data_server) => {
                let cmd = tinyiothub_core::models::thing_command::ThingCommand {
                    id: uuid::Uuid::new_v4().to_string(),
                    thing_id: input.thing_id.clone(),
                    name: input.action_name.clone(),
                    display_name: None,
                    description: None,
                    parameters: input.params.as_ref().map(|p| p.to_string()),
                    created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                };
                match data_server.execute_command(cmd) {
                    Ok(()) => tool_ok(json!({
                        "thingId": input.thing_id,
                        "actionName": input.action_name,
                        "status": "executed",
                        "message": "操作已下发执行"
                    })),
                    Err(e) => tool_err(format!("操作执行失败: {}", e)),
                }
            }
            None => {
                tracing::warn!("DataServer not available, action execution is simulated");
                tool_ok(json!({
                    "thingId": input.thing_id,
                    "actionName": input.action_name,
                    "status": "simulated",
                    "message": "操作已记录（DataServer 未就绪，实际执行已模拟）"
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domains::agent::host::test_utils::seed_test_workspace;
    use crate::shared::access_ports::{
        DbAcceptanceStore, DbDeviceStore, DbDriverStore, DbProjectionStore, DbThingSource,
    };
    use tinyiothub_agent::runtime::thing_agent::{ResourceMode, ResourceSpec};
    use tinyiothub_core::driver::declaration::{DeviceDeclaration, DeviceDeclarationRecord};
    use tinyiothub_runtime::access::demand::DemandTracker;
    use tinyiothub_runtime::access::registry::ManifestRegistry;
    use tinyiothub_runtime::access::service::AccessService;
    use tinyiothub_runtime::access::validator::AccessCallValidator;

    /// 包级资源声明：actuator exclusive、camera exclusive。
    const RESOURCE_MANIFEST: &str = r#"{
      "manifestVersion":"1.0",
      "identity":{"driver":"resource-demo","version":"1.0.0","protocol":"demo",
                  "abiVersion":2,"dynamicCapabilities":true},
      "config":{"type":"object"},"tools":[],
      "stateFields":[{"name":"level","type":"integer"}],
      "sources":[],
      "resources":[
        {"id":"actuator","mode":"exclusive","capacity":1},
        {"id":"camera","mode":"exclusive","capacity":1}
      ],
      "placement":{"mode":"runtime"},
      "limits":{"maxConcurrentJobs":1,"maxCallsPerMin":60}
    }"#;

    /// 真 AccessService：声明写入 store，包级 resources 来自内置 manifest。
    async fn resource_access(pool: &SqlitePool, ws: &str, thing_id: &str, tools: Value) -> Arc<AccessService> {
        let db = Arc::new(tinyiothub_storage::Db::new(pool.clone()));
        let manifests = Arc::new(ManifestRegistry::new());
        manifests.seed_builtin(RESOURCE_MANIFEST).expect("resource manifest");
        let access = Arc::new(AccessService::new(
            std::path::PathBuf::from("."),
            manifests,
            Arc::new(DbDriverStore(db.clone())),
            Arc::new(DbDeviceStore(db.clone())),
            Arc::new(DbProjectionStore(db.clone())),
            Arc::new(DbThingSource(db.clone())),
            Arc::new(DbAcceptanceStore(db.clone())),
            Arc::new(DemandTracker::new()),
        ));
        let decl: DeviceDeclaration = serde_json::from_value(json!({
            "manifestVersion": "1.0",
            "driver": "resource-demo",
            "driverVersion": "1.0.0",
            "stateFields": [{"name":"level","type":"integer"}],
            "tools": tools,
        }))
        .expect("declaration json");
        db.upsert_device_declaration(&DeviceDeclarationRecord {
            workspace_id: ws.into(),
            thing_id: thing_id.into(),
            driver_name: decl.driver.clone(),
            driver_version: decl.driver_version.clone(),
            declaration_json: serde_json::to_string(&decl).unwrap(),
            declaration_hash: "test-hash".into(),
            status: "active".into(),
            updated_at: "2026-10-22T00:00:00Z".into(),
        })
        .await
        .expect("declaration row");
        access
    }

    fn tool_with_access(
        pool: &SqlitePool,
        ws: &str,
        registry: Arc<ArbitrationRegistry>,
        holder: &str,
        access: Arc<AccessService>,
    ) -> InvokeActionTool {
        let mut tool = tool(pool, ws, Some(registry), holder);
        tool.access = Some(access.clone() as Arc<dyn AccessCallValidator>);
        tool.resources = Some(access as Arc<dyn ResourceResolver>);
        tool
    }

    async fn register_action(pool: &SqlitePool, thing_id: &str, name: &str) {
        sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES (?, ?, ?)")
            .bind(format!("act-{name}-{thing_id}"))
            .bind(thing_id)
            .bind(name)
            .execute(pool)
            .await
            .expect("register action");
    }

    fn exclusive(id: &str) -> ResourceSpec {
        ResourceSpec {
            id: id.into(),
            mode: ResourceMode::Exclusive,
            capacity: 1,
        }
    }

    async fn test_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        tinyiothub_storage::migrations::run_migrations(&pool)
            .await
            .expect("migrations");
        pool
    }

    /// 一台 device + 一个已注册动作 + 关闭确认门（仲裁在执行前——绕过
    /// 确认分支，直达执行路径）。
    async fn seed_device(pool: &SqlitePool, ws: &str, thing_id: &str) {
        seed_test_workspace(pool, "tenant-1", ws).await;
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, ?, 'device')")
            .bind(thing_id)
            .bind(format!("Thing {thing_id}"))
            .bind(ws)
            .execute(pool)
            .await
            .expect("insert device");
        sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES (?, ?, ?)")
            .bind(format!("act-reboot-{thing_id}"))
            .bind(thing_id)
            .bind("reboot")
            .execute(pool)
            .await
            .expect("register action");
        sqlx::query("UPDATE workspaces SET require_action_confirm = 0 WHERE id = ?")
            .bind(ws)
            .execute(pool)
            .await
            .expect("disable confirm gate");
    }

    fn tool(
        pool: &SqlitePool,
        ws: &str,
        arbitration: Option<Arc<ArbitrationRegistry>>,
        holder: &str,
    ) -> InvokeActionTool {
        InvokeActionTool {
            thing_service: Arc::new(ThingService::new(pool.clone())),
            pool: pool.clone(),
            workspace_id: ws.to_string(),
            data_server: None,
            pending_actions: Arc::new(PendingActionStore::new()),
            arbitration,
            arbitration_holder: holder.to_string(),
            approval: None,
            access: None,
            resources: None,
        }
    }

    /// 两 run 并发同设备：run-A 持锁期间 run-B 的 invoke_action 被仲裁
    /// 拒绝且 busy 文案带持有者名；run-A 结束（Lease drop）后 run-B 放行。
    #[tokio::test]
    async fn concurrent_runs_same_device_second_gets_busy_with_holder() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-arb", "dev-1").await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let challenger = tool(&pool, "ws-arb", Some(registry.clone()), "watch-run-B");

        // run-A 在飞（持锁）。
        let lease_a = registry
            .try_acquire_default("dev-1", "watch-run-A".to_string())
            .expect("run-A acquire");

        let busy = challenger
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(!busy.success, "run-B 必须被仲裁拒绝: {busy:?}");
        let msg = busy.error.unwrap_or_default();
        assert!(
            msg.contains("watch-run-A"),
            "busy 必须带持有者名（人读得懂的拒绝）: {msg}"
        );

        // run-A 结束释放 → run-B 可执行（无 DataServer → simulated）。
        drop(lease_a);
        let ok = challenger
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(ok.success, "释放后必须放行: {:?}", ok.error);
        assert!(ok.output.contains("simulated"), "无 DataServer 走模拟: {ok:?}");
    }

    /// 未接线仲裁注册表（None）时行为不变——即使他人持锁也照常执行。
    #[tokio::test]
    async fn no_registry_keeps_legacy_behavior() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-legacy", "dev-1").await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let _held = registry
            .try_acquire_default("dev-1", "someone-else".to_string())
            .expect("held");
        let unwired = tool(&pool, "ws-legacy", None, "chat");
        let ok = unwired
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(ok.success, "未接线仲裁时行为不变: {:?}", ok.error);
    }

    // ── E4c：声明资源 claim ─────────────────────────────

    /// 声明了 resources 的 action：claim 该资源；他人持有时 busy 带
    /// holder + resource（spec §2.4）。
    #[tokio::test]
    async fn declared_resources_are_claimed_and_busy_carries_holder_and_resource() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-res", "dev-1").await;
        let access = resource_access(
            &pool,
            "ws-res",
            "dev-1",
            json!([{"name":"reboot","kind":"action","parameters":{"type":"object"},"resources":["actuator"]}]),
        )
        .await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = tool_with_access(&pool, "ws-res", registry.clone(), "watch-run-B", access);

        let held = registry
            .try_acquire_resources(
                "dev-1",
                "watch-run-A".into(),
                &[exclusive("actuator")],
                DEFAULT_LEASE_TTL,
            )
            .expect("run-A 占 actuator");
        let busy = tool
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(!busy.success, "同资源被占必须 busy");
        let msg = busy.error.unwrap_or_default();
        assert!(msg.contains("busy:"), "busy 文案: {msg}");
        assert!(msg.contains("watch-run-A"), "busy 必须带持有者: {msg}");
        assert!(msg.contains("actuator"), "busy 必须带资源 id: {msg}");

        // 释放后同一工具可执行。
        drop(held);
        let ok = tool
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(ok.success, "释放后必须放行: {:?}", ok.error);
    }

    /// 不同声明资源互不冲突：actuator 被占不影响 camera 上的 action。
    #[tokio::test]
    async fn different_declared_resources_do_not_conflict() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-res2", "dev-1").await;
        register_action(&pool, "dev-1", "calibrate").await;
        let access = resource_access(
            &pool,
            "ws-res2",
            "dev-1",
            json!([
                {"name":"reboot","kind":"action","parameters":{"type":"object"},"resources":["actuator"]},
                {"name":"calibrate","kind":"action","parameters":{"type":"object"},"resources":["camera"]}
            ]),
        )
        .await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = tool_with_access(&pool, "ws-res2", registry.clone(), "chat", access);

        let _held = registry
            .try_acquire_resources(
                "dev-1",
                "watch-run-A".into(),
                &[exclusive("actuator")],
                DEFAULT_LEASE_TTL,
            )
            .expect("run-A 占 actuator");
        let ok = tool
            .execute(json!({"thingId": "dev-1", "actionName": "calibrate"}))
            .await
            .expect("execute");
        assert!(ok.success, "不同资源必须可并行: {:?}", ok.error);
    }

    /// 未声明 resources 的 action 仍 claim 合成 `__device__`（E1 语义）。
    #[tokio::test]
    async fn action_with_empty_resources_claims_device_resource() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-dev", "dev-1").await;
        register_action(&pool, "dev-1", "echo").await;
        let access = resource_access(
            &pool,
            "ws-dev",
            "dev-1",
            json!([{"name":"echo","kind":"action","parameters":{"type":"object"},"resources":[]}]),
        )
        .await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = tool_with_access(&pool, "ws-dev", registry.clone(), "chat", access);

        let _held = registry
            .try_acquire_default("dev-1", "someone-else".to_string())
            .expect("held");
        let busy = tool
            .execute(json!({"thingId": "dev-1", "actionName": "echo"}))
            .await
            .expect("execute");
        assert!(!busy.success, "空声明必须回落到设备互斥");
        let msg = busy.error.unwrap_or_default();
        assert!(msg.contains("someone-else"), "busy 带持有者: {msg}");
        assert!(msg.contains("__device__"), "busy 带合成资源: {msg}");
    }

    /// 未知资源 id fail-closed：拒绝且不留下 partial claim。
    #[tokio::test]
    async fn unknown_resource_is_rejected_before_claim() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-ghost", "dev-1").await;
        register_action(&pool, "dev-1", "ghost").await;
        let access = resource_access(
            &pool,
            "ws-ghost",
            "dev-1",
            json!([{"name":"ghost","kind":"action","parameters":{"type":"object"},"resources":["not-declared"]}]),
        )
        .await;
        let registry = Arc::new(ArbitrationRegistry::default());
        let tool = tool_with_access(&pool, "ws-ghost", registry.clone(), "chat", access);

        let failed = tool
            .execute(json!({"thingId": "dev-1", "actionName": "ghost"}))
            .await
            .expect("execute");
        assert!(!failed.success);
        let msg = failed.error.unwrap_or_default();
        assert!(
            msg.contains("unknown resource 'not-declared'"),
            "fail-closed 文案: {msg}"
        );

        // 解析失败发生在 claim 之前：设备资源未被占用。
        registry
            .try_acquire_default("dev-1", "after".to_string())
            .expect("失败解析不得留下 claim");
    }

    /// stop 闸门期间的 claim 返回 busy，holder 带 stop 原因（spec §2.1）。
    #[tokio::test]
    async fn post_stop_claim_is_rejected_with_stop_reason() {
        let pool = test_pool().await;
        seed_device(&pool, "ws-stop", "dev-1").await;
        let access = resource_access(
            &pool,
            "ws-stop",
            "dev-1",
            json!([{"name":"reboot","kind":"action","parameters":{"type":"object"},"resources":["actuator"]}]),
        )
        .await;
        let registry = Arc::new(ArbitrationRegistry::default());
        registry.set_stop(
            "dev-1",
            "operator stop".into(),
            tinyiothub_agent::runtime::thing_agent::arbitration::DEFAULT_STOP_TTL,
        );
        let tool = tool_with_access(&pool, "ws-stop", registry, "chat", access);

        let failed = tool
            .execute(json!({"thingId": "dev-1", "actionName": "reboot"}))
            .await
            .expect("execute");
        assert!(!failed.success, "stop 期间 claim 必须拒绝");
        let msg = failed.error.unwrap_or_default();
        assert!(msg.contains("stop:operator stop"), "busy 必须带 stop 原因: {msg}");
        assert!(msg.contains("__device__"), "stop busy 资源为合成设备资源: {msg}");
    }
}
