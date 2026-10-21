//! E2 会话临时 watch 工具（spec §3.1 / R2-1-2）。
//!
//! `watch` 注册 **one-shot 临时** 规则（persistent=false）：重启/超时即失效。
//! 长期值守必须落目标表（E1c）——这是架构的显式边界，工具描述里对模型写明。

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use tinyiothub_agent::port::attribution::{Attributable, Role, ToolKind};
use tinyiothub_agent::port::tool::{Tool, ToolResult};
use tinyiothub_core::watch::{CompareOp, HealthLevel, WatchCondition, WatchRule};
use tinyiothub_storage::watch::WatchRuleRow;

use super::ManagerSlot;
use super::{tool_err, tool_ok};

/// 与 `handler/watch.rs` 的 API 限额一致（spec §2.2）。
const MAX_ACTIVE_WATCHES: usize = 20;
const DEFAULT_TIMEOUT_SECS: u32 = 600;

pub(super) struct WatchTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
    pub(super) manager: ManagerSlot,
}

impl Attributable for WatchTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for WatchTool {
    fn name(&self) -> &str {
        "watch"
    }

    fn description(&self) -> &str {
        "注册会话临时 watch（one-shot）：条件命中时唤醒本会话一次。\
         三种条件（三选一）：①property：thingId+property+op(lt/gt/eq/ne)+value；\
         ②health：thingId+level(attention/bad)；③availability：thingId+online。\
         临时 watch 不持久——重启或超时即失效；需要长期值守请设目标。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "thingId": { "type": "string", "description": "物 ID（必需）" },
                "property": { "type": "string", "description": "属性名（property 条件）" },
                "op": { "type": "string", "enum": ["lt", "gt", "eq", "ne"] },
                "value": { "description": "比较目标值（property 条件）" },
                "level": { "type": "string", "enum": ["attention", "bad"] },
                "online": { "type": "boolean" },
                "timeoutSecs": { "type": "integer", "description": "超时秒（默认 600，上限 3600）" }
            },
            "required": ["thingId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "thing_id")]
            thing_id: String,
            property: Option<String>,
            op: Option<String>,
            value: Option<Value>,
            level: Option<String>,
            online: Option<bool>,
            #[serde(alias = "timeout_secs")]
            timeout_secs: Option<u32>,
        }

        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {e}"))?;

        let condition = match (&input.property, &input.level, input.online) {
            (Some(property), None, None) => {
                let op = match input.op.as_deref() {
                    Some("lt") => CompareOp::Lt,
                    Some("gt") => CompareOp::Gt,
                    Some("eq") => CompareOp::Eq,
                    Some("ne") => CompareOp::Ne,
                    other => return tool_err(format!("property 条件需要 op(lt/gt/eq/ne)，收到 {other:?}")),
                };
                let Some(value) = input.value.clone() else {
                    return tool_err("property 条件需要 value".to_string());
                };
                WatchCondition::Property {
                    thing_id: input.thing_id.clone(),
                    property: property.clone(),
                    op,
                    value,
                }
            }
            (None, Some(level), None) => {
                let level = match level.as_str() {
                    "attention" => HealthLevel::Attention,
                    "bad" => HealthLevel::Bad,
                    other => return tool_err(format!("level 必须是 attention/bad，收到 {other}")),
                };
                WatchCondition::Health {
                    thing_id: input.thing_id.clone(),
                    level,
                }
            }
            (None, None, Some(online)) => WatchCondition::Availability {
                thing_id: input.thing_id.clone(),
                online,
            },
            _ => return tool_err("必须且只能指定一种条件：property+op+value / level / online".to_string()),
        };

        // 引用物存在性（本区）。
        let db = tinyiothub_storage::Db::new(self.pool.clone());
        match db.find_thing_by_id(Some(&self.workspace_id), &input.thing_id).await {
            Ok(Some(_)) => {}
            Ok(None) => return tool_err(format!("watch 引用的物不存在：{}", input.thing_id)),
            Err(e) => return tool_err(format!("物查询失败：{e}")),
        }

        // 每工作区活跃 watch 上限。
        let active = match tinyiothub_storage::watch::list_rules(&self.pool).await {
            Ok(rows) => rows.iter().filter(|r| r.workspace_id == self.workspace_id).count(),
            Err(e) => return tool_err(format!("watch 列表读取失败：{e}")),
        };
        if active >= MAX_ACTIVE_WATCHES {
            return tool_err(format!(
                "watch 数量已达上限 {MAX_ACTIVE_WATCHES}（当前工作区）；请先 unwatch"
            ));
        }

        let rule = WatchRule {
            id: format!("wr_{}", uuid::Uuid::new_v4().simple()),
            workspace_id: self.workspace_id.clone(),
            condition,
            persistent: false,
            timeout_secs: input.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(e) = rule.validate() {
            return tool_err(e);
        }

        // 持久源（临时 watch 也入库，靠 one-shot 退役写穿删除——E1 C-2 语义）。
        let condition_json = match serde_json::to_value(&rule.condition) {
            Ok(v) => v,
            Err(e) => return tool_err(format!("条件序列化失败：{e}")),
        };
        let kind = condition_json
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("property")
            .to_string();
        let row = WatchRuleRow {
            id: rule.id.clone(),
            workspace_id: rule.workspace_id.clone(),
            kind,
            condition: condition_json.to_string(),
            persistent: 0,
            timeout_secs: i64::from(rule.timeout_secs),
            created_at: rule.created_at.clone(),
        };
        if let Err(e) = tinyiothub_storage::watch::insert_rule(&self.pool, &row).await {
            return tool_err(format!("watch 注册失败：{e}"));
        }
        // 内存注册表（best-effort：loop 未起时下次 start 重织自愈）。
        self.manager.upsert(&self.workspace_id, rule.clone());

        tool_ok(json!({
            "v": 1,
            "ruleId": rule.id,
            "expiresInSecs": rule.timeout_secs,
            "persistent": false,
        }))
    }
}

pub(super) struct UnwatchTool {
    pub(super) pool: SqlitePool,
    pub(super) workspace_id: String,
    pub(super) manager: ManagerSlot,
}

impl Attributable for UnwatchTool {
    fn role(&self) -> Role {
        Role::Tool(ToolKind::Plugin)
    }
    fn alias(&self) -> &str {
        self.name()
    }
}

#[async_trait]
impl Tool for UnwatchTool {
    fn name(&self) -> &str {
        "unwatch"
    }

    fn description(&self) -> &str {
        "取消一个会话临时 watch（ruleId 来自 watch 工具的返回）。"
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "ruleId": { "type": "string", "description": "watch 规则 ID（必需）" }
            },
            "required": ["ruleId"]
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Input {
            #[serde(alias = "rule_id")]
            rule_id: String,
        }
        let input: Input = serde_json::from_value(args).map_err(|e| anyhow::anyhow!("参数解析失败: {e}"))?;

        match tinyiothub_storage::watch::delete_rule(&self.pool, &self.workspace_id, &input.rule_id).await {
            Ok(0) => tool_err(format!("watch 不存在：{}", input.rule_id)),
            Ok(_) => {
                self.manager.remove(&self.workspace_id, &input.rule_id);
                tool_ok(json!({ "v": 1, "status": "removed", "ruleId": input.rule_id }))
            }
            Err(e) => tool_err(format!("watch 删除失败：{e}")),
        }
    }
}
