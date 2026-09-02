//! SceneInstantiator — 场景包实例化：展开（纯函数）→ 配额校验 → 单事务落库。
//!
//! 数据流：
//!   template row ──▶ SceneTemplateFile::from_json(device_info)
//!        │ 收集 template_ref/scene_ref ──▶ find_thing_template_by_name (workspace→builtin)
//!        ▼
//!   expand() ──▶ ExpansionResult (nodes 拓扑序, tree_preview, warnings)
//!        │
//!        ▼
//!   配额校验 (count_things_by_workspace + node_count ≤ thing_limit)
//!        │ dry_run=true → 直接返回（只读）
//!        ▼
//!   单事务：create_thing_row_with_type → properties/commands/resources/alarm_rules → linked_data
//!        │ 名称冲突: resolve_thing_name_tx（快路径）+ 唯一约束捕获重探测（兜底，≤10）
//!        │ 锁竞争（SQLite 单写者）: 整事务回滚重试（≤5）
//!        ▼
//!   commit / rollback（任何失败整体回滚，不留半棵树）
//!
//! 可观测性：项目当前无 metrics 注册表，v1 用 tracing 结构化日志承载
//!（`#[instrument]` + info!/warn! 带 template/node_count/result 字段，
//! 字段口径按 scene_instantiations_total{template, result} 可聚合设计）。

use std::collections::HashMap;

use sqlx::{Sqlite, Transaction};
use tinyiothub_core::models::thing::CreateThingRequest;
use tinyiothub_core::models::thing_command::CreateThingCommandRequest;
use tinyiothub_core::models::thing_property::CreateThingPropertyRequest;
use tinyiothub_storage::Db;
use tinyiothub_storage::alarm::AlarmLevel;
use tinyiothub_storage::alarm_rule::{AlarmCondition, AlarmRule, NotificationConfig, RuleType};
use tinyiothub_storage::scene_template::{
    ExpandedNode, ExpansionResult, SceneTemplateFile, ThingNodeDef, expand, localized,
};
use tinyiothub_storage::thing_template::ThingTemplate;
use tracing::{info, instrument, warn};

use super::error::{MarketplaceError, Result};

/// 唯一约束兜底重试上限（TOCTOU：SELECT 探测与 INSERT 之间被并发抢占）。
const MAX_NAME_RETRIES: usize = 10;
/// SQLite 单写者锁竞争下的整事务重试上限。
const MAX_TX_RETRIES: usize = 5;

#[derive(Debug)]
pub struct InstantiateParams {
    pub scene_name: String,
    pub parent_id: Option<String>,
    pub parameter_values: HashMap<String, i64>,
    pub dry_run: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstantiateOutcome {
    pub node_count: usize,
    pub root_thing_id: Option<String>,
    pub tree_preview: String,
    pub warnings: Vec<String>,
}

pub struct SceneInstantiator;

impl SceneInstantiator {
    #[instrument(skip(db, params), fields(dry_run = params.dry_run))]
    pub async fn instantiate(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
    ) -> Result<InstantiateOutcome> {
        // 1. 加载模板
        let template = db
            .find_thing_template_by_id(template_id, workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?
            .ok_or_else(|| MarketplaceError::NotFound(format!("模板不存在: {}", template_id)))?;
        if !template.is_composition() {
            return Err(MarketplaceError::InvalidConfig(
                "非场景包模板，请使用 install 接口".to_string(),
            ));
        }
        let scene = SceneTemplateFile::from_json(&template.device_info)
            .map_err(|e| MarketplaceError::Template(format!("场景包解析失败: {}", e)))?;

        // 2. 收集并加载引用（workspace → builtin）
        let (device_refs, scene_refs) = collect_refs(&scene);
        let mut device_templates: HashMap<String, ThingTemplate> = HashMap::new();
        for name in &device_refs {
            let t = db
                .find_thing_template_by_name(name, workspace_id)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?
                .ok_or_else(|| MarketplaceError::Template(format!("引用模板不存在或已停用: {}", name)))?;
            device_templates.insert(name.clone(), t);
        }
        let mut scene_templates: HashMap<String, SceneTemplateFile> = HashMap::new();
        for name in &scene_refs {
            let t = db
                .find_thing_template_by_name(name, workspace_id)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?
                .ok_or_else(|| MarketplaceError::Template(format!("引用场景包不存在或已停用: {}", name)))?;
            scene_templates.insert(
                name.clone(),
                SceneTemplateFile::from_json(&t.device_info)
                    .map_err(|e| MarketplaceError::Template(format!("场景包 {} 解析失败: {}", name, e)))?,
            );
        }

        // 3. 展开（纯函数）
        let result = expand(
            &scene,
            &params.scene_name,
            &params.parameter_values,
            &device_templates,
            &scene_templates,
        )
        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;

        // 4. 配额校验（真实行数，不用缓存计数）
        let current = db
            .count_things_by_workspace(workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;
        let limit = db
            .tenant_thing_limit(workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(format!("配额查询失败: {}", e)))?;
        if current + result.node_count as i64 > limit {
            return Err(MarketplaceError::Validation(format!(
                "超出配额：当前 {} 个本体 + 将创建 {} 个 > 上限 {}",
                current, result.node_count, limit
            )));
        }

        // 5. dry-run：只读返回
        if params.dry_run {
            info!(node_count = result.node_count, template = %template.name, result = "dry_run", "场景包 dry-run 预览完成");
            return Ok(InstantiateOutcome {
                node_count: result.node_count,
                root_thing_id: None,
                tree_preview: result.tree_preview,
                warnings: result.warnings,
            });
        }

        // 6. parent_id 校验（存在且属于本 workspace）
        if let Some(parent_id) = &params.parent_id {
            let parent = db
                .find_thing_by_id(Some(workspace_id), parent_id)
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?;
            if parent.is_none() {
                return Err(MarketplaceError::Validation(format!(
                    "父本体不存在或不属于当前 workspace: {}",
                    parent_id
                )));
            }
        }

        // 7. 单事务落库；SQLite 单写者锁竞争时整体回滚重试
        let mut attempt = 0usize;
        let (root_id, warnings) = loop {
            let mut tx = db
                .pool()
                .begin()
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?;
            match persist_tree(db, &mut tx, workspace_id, template_id, params, &result).await {
                Ok((root_id, mut persist_warnings)) => {
                    tx.commit()
                        .await
                        .map_err(|e| MarketplaceError::Template(format!("提交事务失败: {}", e)))?;
                    persist_warnings.extend(result.warnings.clone());
                    break (root_id, persist_warnings);
                }
                Err(e) if is_lock_contention(&e) && attempt < MAX_TX_RETRIES => {
                    attempt += 1;
                    warn!(attempt, error = %e, template = %template.name, "实例化遇到锁竞争，回滚后重试");
                    drop(tx); // 显式回滚
                    tokio::time::sleep(std::time::Duration::from_millis(50 * attempt as u64)).await;
                    continue;
                }
                Err(e) => {
                    warn!(error = %e, template = %template.name, result = "failure", "场景包实例化失败，事务回滚");
                    // tx drop 自动回滚
                    return Err(e);
                }
            }
        };

        info!(
            node_count = result.node_count,
            template = %template.name,
            result = "success",
            "场景包实例化完成"
        );
        Ok(InstantiateOutcome {
            node_count: result.node_count,
            root_thing_id: Some(root_id),
            tree_preview: result.tree_preview,
            warnings,
        })
    }
}

/// 递归收集 template_ref / scene_ref 引用名（去重排序，加载顺序确定性）。
fn collect_refs(scene: &SceneTemplateFile) -> (Vec<String>, Vec<String>) {
    fn walk(nodes: &[ThingNodeDef], device: &mut Vec<String>, scene: &mut Vec<String>) {
        for n in nodes {
            if let Some(r) = &n.template_ref {
                device.push(r.clone());
            }
            if let Some(r) = &n.scene_ref {
                scene.push(r.clone());
            }
            walk(&n.children, device, scene);
        }
    }
    let mut device = Vec::new();
    let mut scenes = Vec::new();
    walk(&scene.children, &mut device, &mut scenes);
    device.sort();
    device.dedup();
    scenes.sort();
    scenes.dedup();
    (device, scenes)
}

/// 事务内落库：拓扑序创建本体 → 子表。返回 (root_thing_id, warnings)。
/// 任何一步失败即返回 Err，由调用方回滚整个事务。
async fn persist_tree(
    db: &Db,
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    template_id: &str,
    params: &InstantiateParams,
    result: &ExpansionResult,
) -> Result<(String, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut real_ids: HashMap<usize, String> = HashMap::new();
    let mut root_id = String::new();

    for node in &result.nodes {
        let real_parent = match node.parent_temp_id {
            Some(pid) => real_ids.get(&pid).cloned(),
            None => params.parent_id.clone(),
        };

        // 名称冲突：SELECT 探测（快路径）+ 唯一约束捕获重探测（TOCTOU 兜底，≤10）
        let mut tries = 0usize;
        let (resolved, id) = loop {
            let resolved = db
                .resolve_thing_name_tx(tx, workspace_id, &node.name)
                .await
                .map_err(|e| MarketplaceError::Validation(e.to_string()))?;
            let req = build_thing_request(node, resolved.clone(), real_parent.clone(), template_id, workspace_id);
            match db.create_thing_row_with_type_tx(tx, &req, &node.thing_type).await {
                Ok(id) => break (resolved, id),
                Err(e) if is_unique_violation(&e) && tries < MAX_NAME_RETRIES => {
                    tries += 1;
                    continue;
                }
                Err(e) => return Err(MarketplaceError::Template(format!("创建本体失败: {}", e))),
            }
        };
        if resolved != node.name {
            warnings.push(format!("名称冲突：{} → {}", node.name, resolved));
        }

        real_ids.insert(node.temp_id, id.clone());
        if node.temp_id == 0 {
            root_id = id.clone();
        }

        persist_children_tables(db, tx, workspace_id, &id, node, &mut warnings).await?;
    }
    Ok((root_id, warnings))
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(dbe) if dbe.is_unique_violation())
}

/// SQLite 锁竞争（database/table locked、busy）判定：整事务回滚重试的信号。
fn is_lock_contention(e: &MarketplaceError) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("locked") || msg.contains("busy")
}

fn build_thing_request(
    node: &ExpandedNode,
    resolved_name: String,
    parent_id: Option<String>,
    template_id: &str,
    workspace_id: &str,
) -> CreateThingRequest {
    // linked_data：knowledge / event_defs / dashboard 按顶层键合并（v1 新建，无既有键）
    let mut linked = serde_json::Map::new();
    if let Some(k) = &node.knowledge {
        linked.insert("knowledge".to_string(), serde_json::json!(k));
    }
    if !node.event_defs.is_empty() {
        linked.insert("event_defs".to_string(), serde_json::json!(node.event_defs));
    }
    if let Some(d) = &node.dashboard {
        linked.insert("dashboard".to_string(), d.clone());
    }
    let linked_data = if linked.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(linked).to_string())
    };

    CreateThingRequest {
        name: resolved_name,
        display_name: node.display_name.clone(),
        category: Some(node.category.clone()),
        address: None,
        description: None,
        position: None,
        driver_name: None,
        device_model: None,
        protocol_type: None,
        factory_name: None,
        linked_data,
        driver_options: None,
        parent_id,
        linked_gateway: None,
        fingerprint: None,
        template_id: Some(template_id.to_string()),
        workspace_id: Some(workspace_id.to_string()),
    }
}

async fn persist_children_tables(
    db: &Db,
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    thing_id: &str,
    node: &ExpandedNode,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // 属性
    let props: Vec<CreateThingPropertyRequest> = node
        .properties
        .iter()
        .map(|p| CreateThingPropertyRequest {
            thing_id: thing_id.to_string(),
            name: p.name.clone(),
            display_name: localized(&p.display_name),
            description: p.description.as_ref().and_then(localized),
            data_type: Some(p.data_type.clone()),
            unit: p.unit.clone(),
            min_value: p.min_value,
            max_value: p.max_value,
            default_value: p.default_value.clone(),
            is_read_only: Some(p.is_read_only as i32),
        })
        .collect();
    db.create_thing_properties_batch_tx(tx, &props)
        .await
        .map_err(|e| MarketplaceError::Template(format!("创建属性失败: {}", e)))?;

    // 命令
    let cmds: Vec<CreateThingCommandRequest> = node
        .commands
        .iter()
        .map(|c| CreateThingCommandRequest {
            thing_id: thing_id.to_string(),
            name: c.name.clone(),
            display_name: localized(&c.display_name),
            description: c.description.as_ref().and_then(localized),
            parameters: c.parameters.clone(),
        })
        .collect();
    db.bulk_create_thing_commands_tx(tx, &cmds)
        .await
        .map_err(|e| MarketplaceError::Template(format!("创建命令失败: {}", e)))?;

    // 资源（file_path = uri 原样记录，v1 无真实托管）
    for r in &node.resources {
        db.insert_thing_resource_tx(tx, workspace_id, thing_id, &r.resource_type, &r.name, &r.uri)
            .await
            .map_err(|e| MarketplaceError::Template(format!("创建资源失败: {}", e)))?;
    }

    // 告警规则：property_ref → 本节点真实 property_id
    for rule in &node.alarm_rules {
        let property_id = match &rule.property_ref {
            Some(ref_name) => {
                let found = db
                    .find_thing_property_id_by_name_tx(tx, thing_id, ref_name)
                    .await
                    .map_err(|e| MarketplaceError::Template(e.to_string()))?;
                match found {
                    Some(id) => Some(id),
                    None => {
                        warnings.push(format!("告警规则 {} 引用的属性 {} 不存在，跳过", rule.name, ref_name));
                        continue;
                    }
                }
            }
            None => None,
        };
        let alarm = AlarmRule::new(
            rule.name.clone(),
            rule.description.clone(),
            Some(thing_id.to_string()),
            property_id,
            parse_rule_type(&rule.rule_type)?,
            serde_json::from_value::<AlarmCondition>(rule.condition.clone())
                .map_err(|e| MarketplaceError::Validation(format!("告警条件格式错误: {}", e)))?,
            parse_alarm_level(&rule.alarm_level),
            serde_json::from_value::<NotificationConfig>(rule.notification_config.clone()).unwrap_or_default(),
            workspace_id.to_string(),
        )
        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;
        db.create_alarm_rule_tx(tx, &alarm)
            .await
            .map_err(|e| MarketplaceError::Template(format!("创建告警规则失败: {}", e)))?;
    }
    Ok(())
}

/// rule_type 字符串 → RuleType。展开器已校验 4 个允许值，此处再校验一次兜底。
fn parse_rule_type(s: &str) -> Result<RuleType> {
    match s {
        "threshold" => Ok(RuleType::Threshold),
        "range" => Ok(RuleType::Range),
        "change" => Ok(RuleType::Change),
        "event" => Ok(RuleType::Event),
        other => Err(MarketplaceError::Validation(format!("不支持的告警规则类型: {}", other))),
    }
}

/// alarm_level 字符串 → AlarmLevel；未知值降级为 warning。
fn parse_alarm_level(s: &str) -> AlarmLevel {
    match s {
        "info" => AlarmLevel::Info,
        "error" => AlarmLevel::Error,
        "critical" => AlarmLevel::Critical,
        _ => AlarmLevel::Warning,
    }
}
