//! E4b 采样需求快照（spec 2026-10-21-e4b-sampling-hub-stream §2.1）。
//!
//! 持久规则 → 需要采样的 `(workspace, thing, property)` 集合：
//! - `watch_rules`：property 条件取 `condition.thing_id/property`；
//!   health/availability 只需求整个物（`property = ""`）。
//! - 派生属性：`thing_properties.expression IS NOT NULL`，展开 `refs` JSON
//!   （工作区取派生属性所属物的 `things.workspace_id`）。
//! - 启用的告警规则：`thing_alarm_rules`（property_id → `thing_properties.name`）。
//!
//! JSON 解析是防御式的：单行条件/refs 损坏只跳过该行并记日志，不影响整份
//! 快照（sampler 每秒调用，缺一条不应让全部需求失效）。

use std::collections::BTreeSet;

use sqlx::{FromRow, SqlitePool};

use crate::database::Db;

/// 一条持久规则声明的采用需求（db-local 行类型，不依赖 runtime）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DemandPropertyRow {
    pub workspace_id: String,
    pub thing_id: String,
    pub property: String,
}

#[derive(Debug, FromRow)]
struct WatchConditionRow {
    id: String,
    workspace_id: String,
    kind: String,
    condition: String,
}

#[derive(Debug, FromRow)]
struct DerivedRefRow {
    thing_id: String,
    workspace_id: Option<String>,
    refs: String,
}

#[derive(Debug, FromRow)]
struct AlarmPropertyRow {
    workspace_id: Option<String>,
    thing_id: String,
    property: String,
}

/// watch_rules → 需求行。health/availability 的 `property` 为空串。
pub(crate) async fn list_watch_property_refs(pool: &SqlitePool) -> Result<Vec<DemandPropertyRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, WatchConditionRow>("SELECT id, workspace_id, kind, condition FROM watch_rules")
        .fetch_all(pool)
        .await?;

    let mut out = Vec::new();
    for row in rows {
        let condition: serde_json::Value = match serde_json::from_str(&row.condition) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(rule_id = %row.id, error = %e, "demand snapshot: 跳过损坏的 watch 条件");
                continue;
            }
        };
        let Some(thing_id) = condition.get("thing_id").and_then(|v| v.as_str()) else {
            tracing::warn!(rule_id = %row.id, "demand snapshot: watch 条件缺少 thing_id");
            continue;
        };
        let property = match row.kind.as_str() {
            "property" => match condition.get("property").and_then(|v| v.as_str()) {
                Some(property) => property.to_string(),
                None => {
                    tracing::warn!(rule_id = %row.id, "demand snapshot: property watch 缺少 property");
                    continue;
                }
            },
            // health/availability 只需求物的健康/在线状态，不读具体属性。
            "health" | "availability" => String::new(),
            other => {
                tracing::warn!(rule_id = %row.id, kind = other, "demand snapshot: 未知 watch 类型");
                continue;
            }
        };
        out.push(DemandPropertyRow {
            workspace_id: row.workspace_id,
            thing_id: thing_id.to_string(),
            property,
        });
    }
    Ok(out)
}

/// 派生属性 refs → 需求行（工作区取派生属性所属物）。
pub(crate) async fn list_derived_property_refs(pool: &SqlitePool) -> Result<Vec<DemandPropertyRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, DerivedRefRow>(
        "SELECT tp.thing_id, t.workspace_id, tp.refs
         FROM thing_properties tp
         JOIN things t ON t.id = tp.thing_id
         WHERE tp.expression IS NOT NULL AND tp.refs IS NOT NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::new();
    for row in rows {
        let Some(workspace_id) = row.workspace_id else {
            continue;
        };
        let refs: serde_json::Value = match serde_json::from_str(&row.refs) {
            Ok(value) => value,
            Err(e) => {
                tracing::warn!(thing_id = %row.thing_id, error = %e, "demand snapshot: 跳过损坏的派生 refs");
                continue;
            }
        };
        let Some(refs) = refs.as_array() else {
            tracing::warn!(thing_id = %row.thing_id, "demand snapshot: 派生 refs 不是数组");
            continue;
        };
        for reference in refs {
            let thing_id = reference.get("thing_id").and_then(|v| v.as_str());
            let property = reference.get("property_name").and_then(|v| v.as_str());
            match (thing_id, property) {
                (Some(thing_id), Some(property)) => out.push(DemandPropertyRow {
                    workspace_id: workspace_id.clone(),
                    thing_id: thing_id.to_string(),
                    property: property.to_string(),
                }),
                _ => tracing::warn!(thing_id = %row.thing_id, "demand snapshot: 跳过缺字段的派生 ref"),
            }
        }
    }
    Ok(out)
}

/// 启用的告警规则 → 需求行（仅 property_id 非空的规则）。
pub(crate) async fn list_alarm_property_refs(pool: &SqlitePool) -> Result<Vec<DemandPropertyRow>, sqlx::Error> {
    sqlx::query_as::<_, AlarmPropertyRow>(
        "SELECT COALESCE(r.workspace_id, t.workspace_id) AS workspace_id,
                r.thing_id AS thing_id,
                tp.name AS property
         FROM thing_alarm_rules r
         JOIN thing_properties tp ON tp.id = r.property_id
         LEFT JOIN things t ON t.id = r.thing_id
         WHERE r.is_enabled = 1 AND r.property_id IS NOT NULL AND r.thing_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .map(|rows| {
        rows.into_iter()
            .filter_map(|row| {
                row.workspace_id.map(|workspace_id| DemandPropertyRow {
                    workspace_id,
                    thing_id: row.thing_id,
                    property: row.property,
                })
            })
            .collect()
    })
}

/// 三源并集，按 `(workspace, thing, property)` 去重并稳定排序。
pub(crate) async fn list_demand_property_refs(pool: &SqlitePool) -> Result<Vec<DemandPropertyRow>, sqlx::Error> {
    let mut union: BTreeSet<(String, String, String)> = BTreeSet::new();
    for row in list_watch_property_refs(pool).await? {
        union.insert((row.workspace_id, row.thing_id, row.property));
    }
    for row in list_derived_property_refs(pool).await? {
        union.insert((row.workspace_id, row.thing_id, row.property));
    }
    for row in list_alarm_property_refs(pool).await? {
        union.insert((row.workspace_id, row.thing_id, row.property));
    }
    Ok(union
        .into_iter()
        .map(|(workspace_id, thing_id, property)| DemandPropertyRow {
            workspace_id,
            thing_id,
            property,
        })
        .collect())
}

impl Db {
    /// 当前持久规则声明的全部采样需求（去重、按 workspace/thing/property 排序）。
    pub async fn list_demand_property_refs(&self) -> Result<Vec<DemandPropertyRow>, sqlx::Error> {
        list_demand_property_refs(self.pool()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_tenant_workspace(pool: &SqlitePool, tenant: &str, workspace: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name)
             VALUES ('plan_free', 'free', 'Free')",
        )
        .execute(pool)
        .await
        .expect("plan");
        sqlx::query("INSERT INTO tenants (id, name, slug) VALUES (?, ?, ?)")
            .bind(tenant)
            .bind(tenant)
            .bind(tenant)
            .execute(pool)
            .await
            .expect("tenant");
        sqlx::query(
            "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at)
             VALUES (?, ?, ?, '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z')",
        )
        .bind(workspace)
        .bind(workspace)
        .bind(tenant)
        .execute(pool)
        .await
        .expect("workspace");
    }

    async fn seed_thing(pool: &SqlitePool, thing_id: &str, workspace: &str) {
        sqlx::query("INSERT INTO things (id, name, workspace_id) VALUES (?, ?, ?)")
            .bind(thing_id)
            .bind(thing_id)
            .bind(workspace)
            .execute(pool)
            .await
            .expect("thing");
    }

    async fn seed_property(
        pool: &SqlitePool,
        id: &str,
        thing_id: &str,
        name: &str,
        expression: Option<&str>,
        refs: Option<&str>,
    ) {
        sqlx::query(
            "INSERT INTO thing_properties (id, thing_id, name, expression, refs)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(thing_id)
        .bind(name)
        .bind(expression)
        .bind(refs)
        .execute(pool)
        .await
        .expect("thing property");
    }

    async fn seed_watch(pool: &SqlitePool, id: &str, workspace: &str, kind: &str, condition: &str) {
        sqlx::query(
            "INSERT INTO watch_rules (id, workspace_id, kind, condition, persistent, timeout_secs, created_at)
             VALUES (?, ?, ?, ?, 0, 600, '2026-10-21T00:00:00Z')",
        )
        .bind(id)
        .bind(workspace)
        .bind(kind)
        .bind(condition)
        .execute(pool)
        .await
        .expect("watch rule");
    }

    async fn seed_alarm(
        pool: &SqlitePool,
        id: &str,
        workspace: Option<&str>,
        thing_id: &str,
        property_id: &str,
        enabled: bool,
    ) {
        sqlx::query(
            "INSERT INTO thing_alarm_rules
                (id, thing_id, property_id, rule_name, rule_type, condition_config,
                 alarm_level, is_enabled, workspace_id)
             VALUES (?, ?, ?, ?, 'threshold', '{}', 'warning', ?, ?)",
        )
        .bind(id)
        .bind(thing_id)
        .bind(property_id)
        .bind(id)
        .bind(if enabled { 1_i64 } else { 0_i64 })
        .bind(workspace)
        .execute(pool)
        .await
        .expect("alarm rule");
    }

    #[tokio::test]
    async fn demand_snapshot_unions_sources_and_dedups() {
        let pool = crate::test_helpers::test_pool().await;
        seed_tenant_workspace(&pool, "tenant-1", "ws-1").await;
        seed_tenant_workspace(&pool, "tenant-2", "ws-2").await;
        seed_thing(&pool, "t-1", "ws-1").await;
        seed_thing(&pool, "t-2", "ws-1").await;
        seed_thing(&pool, "t-3", "ws-1").await;
        seed_thing(&pool, "t-9", "ws-2").await;

        // property watch（重复一条验证去重）
        seed_watch(
            &pool,
            "w-prop",
            "ws-1",
            "property",
            r#"{"type":"property","thing_id":"t-1","property":"temp","op":"gt","value":30}"#,
        )
        .await;
        seed_watch(
            &pool,
            "w-prop-dup",
            "ws-1",
            "property",
            r#"{"type":"property","thing_id":"t-1","property":"temp","op":"lt","value":5}"#,
        )
        .await;
        // health watch → property = ""
        seed_watch(
            &pool,
            "w-health",
            "ws-1",
            "health",
            r#"{"type":"health","thing_id":"t-2","level":"bad"}"#,
        )
        .await;
        // 另一工作区的 watch（工作区标记必须原样保留）
        seed_watch(
            &pool,
            "w-other",
            "ws-2",
            "availability",
            r#"{"type":"availability","thing_id":"t-9","online":false}"#,
        )
        .await;

        // 派生属性：refs 展开到 t-3 的 energy
        seed_property(
            &pool,
            "p-derived",
            "t-1",
            "total",
            Some("a + b"),
            Some(r#"[{"thing_id":"t-3","property_name":"energy"}]"#),
        )
        .await;
        seed_property(&pool, "p-humidity", "t-2", "humidity", None, None).await;

        // 启用 + 禁用告警规则（同一属性）
        seed_alarm(&pool, "a-on", Some("ws-1"), "t-2", "p-humidity", true).await;
        seed_alarm(&pool, "a-off", Some("ws-1"), "t-2", "p-humidity", false).await;

        let db = Db::new(pool);
        let rows = db.list_demand_property_refs().await.expect("snapshot");

        let expected = vec![
            DemandPropertyRow {
                workspace_id: "ws-1".into(),
                thing_id: "t-1".into(),
                property: "temp".into(),
            },
            DemandPropertyRow {
                workspace_id: "ws-1".into(),
                thing_id: "t-2".into(),
                property: String::new(),
            },
            DemandPropertyRow {
                workspace_id: "ws-1".into(),
                thing_id: "t-2".into(),
                property: "humidity".into(),
            },
            DemandPropertyRow {
                workspace_id: "ws-1".into(),
                thing_id: "t-3".into(),
                property: "energy".into(),
            },
            DemandPropertyRow {
                workspace_id: "ws-2".into(),
                thing_id: "t-9".into(),
                property: String::new(),
            },
        ];
        assert_eq!(rows, expected);
    }

    #[tokio::test]
    async fn demand_snapshot_skips_malformed_json_rows() {
        let pool = crate::test_helpers::test_pool().await;
        seed_tenant_workspace(&pool, "tenant-1", "ws-1").await;
        seed_thing(&pool, "t-1", "ws-1").await;
        seed_thing(&pool, "t-2", "ws-1").await;
        seed_thing(&pool, "t-3", "ws-1").await;
        seed_property(&pool, "p-bad-alarm", "t-1", "temp", None, None).await;

        // 损坏的 watch JSON、缺字段的 refs、非数组 refs
        seed_watch(&pool, "w-bad", "ws-1", "property", "not-json").await;
        seed_watch(
            &pool,
            "w-good",
            "ws-1",
            "property",
            r#"{"type":"property","thing_id":"t-2","property":"humidity"}"#,
        )
        .await;
        seed_property(&pool, "p-bad-refs", "t-1", "d1", Some("1"), Some("not-json")).await;
        seed_property(
            &pool,
            "p-obj-refs",
            "t-1",
            "d2",
            Some("1"),
            Some(r#"{"thing_id":"t-3"}"#),
        )
        .await;
        seed_property(
            &pool,
            "p-partial-refs",
            "t-3",
            "d3",
            Some("1"),
            Some(r#"[{"thing_id":"t-1"},{"thing_id":"t-3","property_name":"power"}]"#),
        )
        .await;
        seed_alarm(&pool, "a-bad-ws", None, "t-1", "p-bad-alarm", true).await;

        let db = Db::new(pool);
        let rows = db.list_demand_property_refs().await.expect("snapshot");

        // 只有合法 watch / 合法 ref 条目 / 合法告警规则产出需求；损坏行与
        // 缺字段条目被跳过。告警规则 workspace_id 为 NULL 时回落到物的工作区。
        assert_eq!(
            rows,
            vec![
                DemandPropertyRow {
                    workspace_id: "ws-1".into(),
                    thing_id: "t-1".into(),
                    property: "temp".into(),
                },
                DemandPropertyRow {
                    workspace_id: "ws-1".into(),
                    thing_id: "t-2".into(),
                    property: "humidity".into(),
                },
                DemandPropertyRow {
                    workspace_id: "ws-1".into(),
                    thing_id: "t-3".into(),
                    property: "power".into(),
                },
            ]
        );
    }
}
