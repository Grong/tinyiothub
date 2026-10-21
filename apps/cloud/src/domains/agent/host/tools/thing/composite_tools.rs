//! E2 composite 工具（spec 2026-10-21-e2 §6.2）：工具列表恒定两条。
//!
//! 整链一次审批：run_composite 需要确认时 mint token；批准通道消费 token
//! 后带 `confirmationToken` 重新调用本工具执行整链（不再逐步确认）。

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_agent::runtime::thing_agent::seams::composite::{CompositeDef, validate_composite};

use super::PendingActionStore;
use super::{store_pending_action, take_pending_action_reasoned, tool_err, tool_ok};
use crate::domains::agent::host::composite::CompositeExecutor;

pub(super) struct ListCompositesTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
}

impl Attributable for ListCompositesTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Search)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for ListCompositesTool {
    fn name(&self) -> &str {
        "list_composites"
    }

    fn description(&self) -> &str {
        "列出本工作区声明的 composite 动作链（id/name/description/步数）。"
    }

    fn parameters_schema(&self) -> Value {
        json!({ "type": "object", "properties": {} })
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        match tinyiothub_storage::composite::list_composites(&self.pool, &self.workspace_id).await {
            Ok(rows) => {
                let composites: Vec<Value> = rows
                    .iter()
                    .filter_map(|r| {
                        let def: CompositeDef = serde_json::from_str(&r.definition).ok()?;
                        Some(json!({
                            "id": r.id,
                            "name": r.name,
                            "description": r.description,
                            "stepCount": def.steps.len(),
                        }))
                    })
                    .collect();
                tool_ok(json!({ "v": 1, "composites": composites }))
            }
            Err(e) => tool_err(format!("composite 列表读取失败：{e}")),
        }
    }
}

pub(super) struct RunCompositeTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
    pub(super) pending_actions: Arc<PendingActionStore>,
    pub(super) executor: Arc<CompositeExecutor>,
}

impl Attributable for RunCompositeTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for RunCompositeTool {
    fn name(&self) -> &str {
        "run_composite"
    }

    fn description(&self) -> &str {
        "执行一条已声明的 composite 动作链（多设备顺序动作）。\
         整链一次审批；失败会停止并按声明逆序回滚，返回 completed/failed/\
         partial/rolled_back 与每步结果。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "compositeId": { "type": "string", "description": "composite ID（必需）" },
                // 内部字段：仅批准通道使用，模型不填。
                "confirmationToken": { "type": "string", "description": "内部：批准后的确认 token" }
            },
            "required": ["compositeId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "composite_id")]
            composite_id: String,
            #[serde(default, alias = "confirmation_token")]
            confirmation_token: Option<String>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {e}"))?;

        // 1. 加载 + 校验声明。
        let row =
            match tinyiothub_storage::composite::get_composite(&self.pool, &self.workspace_id, &input.composite_id)
                .await
            {
                Ok(Some(r)) => r,
                Ok(None) => return tool_err(format!("composite 不存在：{}", input.composite_id)),
                Err(e) => return tool_err(format!("composite 读取失败：{e}")),
            };
        let def: CompositeDef = match serde_json::from_str(&row.definition) {
            Ok(d) => d,
            Err(e) => return tool_err(format!("composite 声明 JSON 损坏：{e}")),
        };
        if def.workspace_id != self.workspace_id {
            return tool_err("composite 不属于本工作区".to_string());
        }
        if let Err(e) = validate_composite(&def) {
            return tool_err(format!("composite 声明非法：{e}"));
        }
        // 2. 引用物存在性（跨区一律拒绝）。
        for step in &def.steps {
            for op in std::iter::once(&step.op).chain(step.rollback.iter()) {
                match tinyiothub_storage::Db::new(self.pool.clone())
                    .find_thing_by_id(Some(&self.workspace_id), op.thing_id())
                    .await
                {
                    Ok(Some(_)) => {}
                    Ok(None) => return tool_err(format!("composite 引用的物不存在：{}", op.thing_id())),
                    Err(e) => return tool_err(format!("物查询失败：{e}")),
                }
            }
        }

        // 3. 审批：批准通道 token / 工作区确认门。
        if let Some(token) = input.confirmation_token.as_deref() {
            match take_pending_action_reasoned(&self.pending_actions, token) {
                Ok(pending) => {
                    if pending.workspace_id != self.workspace_id
                        || pending.action_name != "run_composite"
                        || pending
                            .params
                            .as_ref()
                            .and_then(|p| p.get("compositeId"))
                            .and_then(|v| v.as_str())
                            != Some(def.id.as_str())
                    {
                        return tool_err("approval unavailable: token 与 composite 不匹配；未执行".to_string());
                    }
                }
                Err(e) => {
                    return tool_err(format!("approval {}: action NOT executed", e.reason_code()));
                }
            }
        } else {
            let require_confirm: bool = tinyiothub_storage::Db::new(self.pool.clone())
                .get_workspace_require_action_confirm(&self.workspace_id)
                .await
                .map(|v| v.unwrap_or(1i32) != 0)
                .unwrap_or(true);
            if require_confirm {
                let token = store_pending_action(
                    &self.pending_actions,
                    String::new(),
                    "run_composite".to_string(),
                    Some(json!({ "compositeId": def.id })),
                    self.workspace_id.clone(),
                );
                return tool_ok(json!({
                    "v": 1,
                    "compositeId": def.id,
                    "status": "confirmation_required",
                    "token": token,
                    "message": "该 composite 需要用户确认后整链执行。",
                }));
            }
        }

        // 4. 执行。
        let result = self.executor.execute(&def).await;
        tool_ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tinyiothub_agent::runtime::thing_agent::{ArbitrationRegistry, DEFAULT_LEASE_TTL};

    async fn setup() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        tinyiothub_storage::test_helpers::run_all_migrations(&pool)
            .await
            .unwrap();
        crate::domains::agent::host::test_utils::seed_test_workspace(&pool, "tenant-1", "ws").await;
        for id in ["t1", "t2"] {
            sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES (?, ?, 'ws', 'device')")
                .bind(id)
                .bind(format!("Thing {id}"))
                .execute(&pool)
                .await
                .unwrap();
        }
        for action in ["close", "open"] {
            sqlx::query("INSERT INTO thing_actions (id, thing_id, name) VALUES (?, 't1', ?)")
                .bind(format!("act-{action}"))
                .bind(action)
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE workspaces SET require_action_confirm = 0 WHERE id = 'ws'")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn insert_def(pool: &SqlitePool, id: &str, steps: Value) {
        let def = json!({
            "id": id,
            "workspace_id": "ws",
            "name": "storm_guard",
            "description": "",
            "v": 1,
            "steps": steps,
        });
        tinyiothub_storage::composite::insert_composite(
            pool,
            &tinyiothub_storage::composite::CompositeRow {
                id: id.into(),
                workspace_id: "ws".into(),
                name: "storm_guard".into(),
                description: "".into(),
                definition: def.to_string(),
                created_at: "2026-10-21T00:00:00Z".into(),
            },
        )
        .await
        .unwrap();
    }

    fn tool(pool: &SqlitePool, pending: Arc<PendingActionStore>) -> RunCompositeTool {
        RunCompositeTool {
            pool: pool.clone(),
            workspace_id: "ws".into(),
            pending_actions: pending,
            executor: Arc::new(crate::domains::agent::host::composite::CompositeExecutor {
                pool: pool.clone(),
                workspace_id: "ws".into(),
                data_server: None,
                event_bus: None,
                arbitration: None,
                holder: "test".into(),
            }),
        }
    }

    async fn ledger_rows(pool: &SqlitePool) -> Vec<Value> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT payload FROM hub_ledger WHERE workspace_id = 'ws' AND kind = 'job'")
                .fetch_all(pool)
                .await
                .unwrap();
        rows.into_iter().map(|(p,)| serde_json::from_str(&p).unwrap()).collect()
    }

    #[tokio::test]
    async fn successful_chain_completes_and_ledgers_each_step() {
        let pool = setup().await;
        insert_def(
            &pool,
            "c1",
            json!([
                {"op": "call", "thing_id": "t1", "action": "close", "params": {},
                 "rollback": {"op": "call", "thing_id": "t1", "action": "open", "params": {}}},
                {"op": "call", "thing_id": "t2", "action": "retract", "params": {}}
            ]),
        )
        .await;

        let out = tool(&pool, Arc::new(PendingActionStore::new()))
            .execute(json!({"compositeId": "c1"}))
            .await
            .unwrap();
        assert!(out.success, "{}", out.error.unwrap_or_default());
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["status"], "completed");
        assert_eq!(v["steps"].as_array().unwrap().len(), 2);

        let rows = ledger_rows(&pool).await;
        // accepted + result per step
        assert_eq!(rows.len(), 4, "rows={rows:?}");
        assert!(rows.iter().any(|r| r["status"] == "accepted"));
        assert!(rows.iter().any(|r| r["status"] == "result"));
        assert!(rows.iter().all(|r| r["compositeId"] == "c1" && r["v"] == 1));
    }

    #[tokio::test]
    async fn failure_rolls_back_completed_steps() {
        let pool = setup().await;
        insert_def(
            &pool,
            "c2",
            json!([
                {"op": "call", "thing_id": "t1", "action": "close", "params": {},
                 "rollback": {"op": "call", "thing_id": "t1", "action": "open", "params": {}}},
                // 不存在的属性 → 第 2 步失败
                {"op": "set", "thing_id": "t2", "properties": {"不存在": 1}}
            ]),
        )
        .await;

        let out = tool(&pool, Arc::new(PendingActionStore::new()))
            .execute(json!({"compositeId": "c2"}))
            .await
            .unwrap();
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["status"], "rolled_back", "{v}");
        assert!(
            v["steps"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["status"] == "rolled_back")
        );
        let rows = ledger_rows(&pool).await;
        assert!(
            rows.iter().any(|r| r["status"] == "result"),
            "rollback must ledger result"
        );
    }

    #[tokio::test]
    async fn confirmation_token_roundtrip_executes_chain() {
        let pool = setup().await;
        sqlx::query("UPDATE workspaces SET require_action_confirm = 1 WHERE id = 'ws'")
            .execute(&pool)
            .await
            .unwrap();
        insert_def(
            &pool,
            "c3",
            json!([{"op": "call", "thing_id": "t1", "action": "close", "params": {}}]),
        )
        .await;
        let pending = Arc::new(PendingActionStore::new());
        let t = tool(&pool, Arc::clone(&pending));

        let first = t.execute(json!({"compositeId": "c3"})).await.unwrap();
        let v: Value = serde_json::from_str(&first.output).unwrap();
        assert_eq!(v["status"], "confirmation_required");
        let token = v["token"].as_str().unwrap().to_string();

        let second = t
            .execute(json!({"compositeId": "c3", "confirmationToken": token}))
            .await
            .unwrap();
        let v2: Value = serde_json::from_str(&second.output).unwrap();
        assert_eq!(v2["status"], "completed", "{v2}");

        // token 一次性：重复使用 → unavailable
        let third = t
            .execute(json!({"compositeId": "c3", "confirmationToken": token}))
            .await
            .unwrap();
        assert!(!third.success);
        assert!(third.error.unwrap().contains("unavailable"));
    }

    #[tokio::test]
    async fn busy_first_step_fails_without_partial_success() {
        let pool = setup().await;
        insert_def(
            &pool,
            "c4",
            json!([{"op": "call", "thing_id": "t1", "action": "close", "params": {}}]),
        )
        .await;
        let arbitration = Arc::new(ArbitrationRegistry::default());
        let _held = arbitration
            .try_acquire("t1", "other-run".into(), DEFAULT_LEASE_TTL)
            .unwrap();
        let mut t = tool(&pool, Arc::new(PendingActionStore::new()));
        t.executor = Arc::new(crate::domains::agent::host::composite::CompositeExecutor {
            pool: pool.clone(),
            workspace_id: "ws".into(),
            data_server: None,
            event_bus: None,
            arbitration: Some(arbitration),
            holder: "test".into(),
        });

        let out = t.execute(json!({"compositeId": "c4"})).await.unwrap();
        let v: Value = serde_json::from_str(&out.output).unwrap();
        assert_eq!(v["status"], "failed");
        assert!(v["steps"][0]["error"].as_str().unwrap().contains("busy"));
    }
}
