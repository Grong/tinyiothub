//! SceneInstantiator — 场景包实例化：展开（纯函数）→ 配额/父校验 → 名称解析 → 单事务落库。
//!
//! 数据流：
//!   template row ──▶ SceneTemplateFile::from_json(device_info)
//!        │ 收集 template_ref/scene_ref ──▶ find_thing_template_by_name (workspace→builtin)
//!        ▼
//!   expand() ──▶ ExpansionResult (nodes 拓扑序, tree_preview, warnings)
//!        │
//!        ▼
//!   配额校验 (count_things_by_workspace + node_count ≤ thing_limit)
//!   parent_id 校验（dry-run 与 commit 一致执行，SELECT 只读）
//!        │
//!        ▼
//!   名称解析（两模式共用 resolve_node_names + thing_name_candidates 同算法同上限）：
//!        │ dry_run=true → pool 只读探测，解析后名称重建 tree_preview 返回（零写入）
//!        ▼
//!   单事务：事务内解析名称 → create_thing_row_with_type → properties/commands/resources/alarm_rules → linked_data
//!        │ 名称冲突: 树内互避 + SELECT 探测（快路径）+ 唯一约束捕获重探测（兜底，≤10）
//!        │ 锁竞争（SQLite 单写者）: 整事务回滚重试（≤5）
//!        ▼
//!   commit / rollback（任何失败整体回滚，不留半棵树）
//!   响应 tree_preview 用落库最终名称重建（预览/响应/DB 三者一致）
//!
//! 可观测性：项目当前无 metrics 注册表，v1 用 tracing 结构化日志承载
//!（入口/出口/失败日志带 template_id、node_count、warnings_count、duration_ms、
//! error_category 稳定短标签，字段口径按 scene_instantiations_total{template, result}
//! 可聚合设计）。

use std::collections::{HashMap, HashSet, VecDeque};

use sqlx::{Sqlite, Transaction};
use tinyiothub_core::models::thing::CreateThingRequest;
use tinyiothub_core::models::thing_command::CreateThingCommandRequest;
use tinyiothub_core::models::thing_property::CreateThingPropertyRequest;
use tinyiothub_storage::Db;
use tinyiothub_storage::alarm::AlarmLevel;
use tinyiothub_storage::alarm_rule::{AlarmCondition, AlarmRule, NotificationConfig, RuleType};
use tinyiothub_storage::scene_template::{
    ExpandError, ExpandedNode, ExpansionResult, MAX_SCENE_REF_DEPTH, SceneTemplateFile, ThingNodeDef, expand,
    localized, sanitize_label,
};
use tinyiothub_storage::thing::thing_name_candidates;
use tinyiothub_storage::thing_template::ThingTemplate;
use tracing::{error, info, instrument, warn};

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
        let started = std::time::Instant::now();
        info!(
            template_id,
            dry_run = params.dry_run,
            parameter_values = ?params.parameter_values,
            scene_name = %params.scene_name,
            parent_id = ?params.parent_id,
            "场景包实例化开始"
        );
        let outcome = Self::do_instantiate(db, workspace_id, template_id, params).await;
        let duration_ms = started.elapsed().as_millis() as u64;
        match &outcome {
            Ok(o) => info!(
                template_id,
                node_count = o.node_count,
                warnings_count = o.warnings.len(),
                duration_ms,
                result = if params.dry_run { "dry_run" } else { "success" },
                "场景包实例化完成"
            ),
            Err(e) => {
                let category = error_category(e);
                if is_client_error(e) {
                    warn!(template_id, error_category = category, error = %e, duration_ms, "场景包实例化失败");
                } else {
                    error!(template_id, error_category = category, error = %e, duration_ms, "场景包实例化失败");
                }
            }
        }
        outcome
    }

    async fn do_instantiate(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
    ) -> Result<InstantiateOutcome> {
        // 0. scene_name 校验：trim 后非空、无控制字符、≤128 字符（防垃圾传播到整棵树每个节点名）
        validate_scene_name(&params.scene_name)?;

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

        // 2. 传递闭包收集并加载引用（workspace → builtin）
        let (device_templates, scene_templates) = preload_refs(db, workspace_id, &scene).await?;

        // 3. 展开（纯函数）
        let result = expand(
            &scene,
            &params.scene_name,
            &params.parameter_values,
            &device_templates,
            &scene_templates,
        )?;

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

        // 5. parent_id 校验（存在且属于本 workspace）；dry-run 与 commit 一致执行（SELECT 只读）
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

        // 6. dry-run：只读名称解析（与落库同算法同上限）→ 解析后名称重建预览，零写入
        if params.dry_run {
            let resolved = resolve_node_names(db, None, workspace_id, &result.nodes).await?;
            let mut warnings = name_warnings(&result.nodes, &resolved);
            warnings.extend(result.warnings.clone());
            let tree_preview = rebuild_tree_preview(&result.nodes, &resolved);
            return Ok(InstantiateOutcome {
                node_count: result.node_count,
                root_thing_id: None,
                tree_preview,
                warnings,
            });
        }

        // 7. 单事务落库；SQLite 单写者锁竞争时整体回滚重试
        let mut attempt = 0usize;
        let (root_id, final_names, warnings) = loop {
            let mut tx = db
                .pool()
                .begin()
                .await
                .map_err(|e| MarketplaceError::Template(e.to_string()))?;
            match persist_tree(db, &mut tx, workspace_id, template_id, params, &result).await {
                Ok((root_id, final_names, mut persist_warnings)) => {
                    tx.commit()
                        .await
                        .map_err(|e| MarketplaceError::Template(format!("提交事务失败: {}", e)))?;
                    persist_warnings.extend(result.warnings.clone());
                    break (root_id, final_names, persist_warnings);
                }
                Err(e) if is_lock_contention(&e) && attempt < MAX_TX_RETRIES => {
                    attempt += 1;
                    warn!(template_id, attempt, error_category = "lock_contention", error = %e, "实例化遇到锁竞争，回滚后重试");
                    drop(tx); // 显式回滚
                    tokio::time::sleep(std::time::Duration::from_millis(50 * attempt as u64)).await;
                    continue;
                }
                Err(e) => {
                    // tx drop 自动回滚；最终失败由 instantiate 出口统一记日志
                    return Err(e);
                }
            }
        };

        Ok(InstantiateOutcome {
            node_count: result.node_count,
            root_thing_id: Some(root_id),
            tree_preview: rebuild_tree_preview(&result.nodes, &final_names),
            warnings,
        })
    }
}

/// scene_name 上限（trim 后字符数）。
const MAX_SCENE_NAME_LEN: usize = 128;

/// scene_name 校验：trim 后非空、不含 C0 控制字符（\n/\r/\t 等）、长度 ≤128。
fn validate_scene_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(MarketplaceError::Validation("scene_name 不能为空".to_string()));
    }
    if name.chars().any(char::is_control) {
        return Err(MarketplaceError::Validation(
            "scene_name 不能包含控制字符（换行/制表符等）".to_string(),
        ));
    }
    if trimmed.chars().count() > MAX_SCENE_NAME_LEN {
        return Err(MarketplaceError::Validation(format!(
            "scene_name 长度不能超过 {} 字符",
            MAX_SCENE_NAME_LEN
        )));
    }
    Ok(())
}

/// 失败日志的稳定分类标签（可聚合）：客户端错误细分，服务器错误归 internal。
fn error_category(e: &MarketplaceError) -> &'static str {
    match e {
        MarketplaceError::Expand(ExpandError::TooLarge { .. }) => "too_large",
        MarketplaceError::Expand(ExpandError::RefNotFound { .. }) => "ref_not_found",
        MarketplaceError::Expand(ExpandError::RefCycle { .. }) => "ref_cycle",
        MarketplaceError::Expand(_) => "validation",
        MarketplaceError::Validation(msg) if msg.starts_with("超出配额") => "quota",
        MarketplaceError::Validation(_) | MarketplaceError::InvalidConfig(_) => "validation",
        MarketplaceError::NotFound(_) => "not_found",
        _ => "internal",
    }
}

/// 客户端错误（4xx）记 warn，服务器错误（5xx）记 error。
fn is_client_error(e: &MarketplaceError) -> bool {
    matches!(
        e,
        MarketplaceError::Expand(_)
            | MarketplaceError::Validation(_)
            | MarketplaceError::InvalidConfig(_)
            | MarketplaceError::NotFound(_)
    )
}

/// 名称解析（dry-run 与 commit 共用步骤）：按拓扑序遍历展开节点，
/// 用 thing_name_candidates 同算法同上限（≤10）探测每个节点名称；
/// 树内先解析节点占用的名称计入避让（兄弟/跨分支同名均互避）。
/// `tx` 为 None 时走 pool 只读探测（dry-run），Some 时走事务内探测（commit）。
async fn resolve_node_names(
    db: &Db,
    mut tx: Option<&mut Transaction<'_, Sqlite>>,
    workspace_id: &str,
    nodes: &[ExpandedNode],
) -> Result<Vec<String>> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut resolved = Vec::with_capacity(nodes.len());
    for node in nodes {
        let mut picked = None;
        for candidate in thing_name_candidates(&node.name) {
            if taken.contains(&candidate) {
                continue;
            }
            let exists = match tx.as_deref_mut() {
                Some(t) => db.thing_name_exists_tx(t, workspace_id, &candidate).await,
                None => db.thing_name_exists(workspace_id, &candidate).await,
            }
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;
            if !exists {
                picked = Some(candidate);
                break;
            }
        }
        let name = picked.ok_or_else(|| {
            MarketplaceError::Validation(format!("同名冲突过多（{}），请手动指定名称", node.name))
        })?;
        taken.insert(name.clone());
        resolved.push(name);
    }
    Ok(resolved)
}

/// 名称变更 warnings：解析后名称 ≠ 展开名称时记录（两模式同一行为）。
fn name_warnings(nodes: &[ExpandedNode], resolved: &[String]) -> Vec<String> {
    nodes
        .iter()
        .zip(resolved)
        .filter(|(n, r)| **r != n.name)
        .map(|(n, r)| format!("名称冲突：{} → {}", n.name, r))
        .collect()
}

/// 用解析后名称重建 tree_preview（格式与展开器一致：`<display_name> (<category>)`、
/// 2 空格缩进、sanitize_label 规则不变）。display_name 等于未解析名称时（默认命名
/// 模式产物）跟随解析后名称，保证用户在树上看到的名字与 DB 一致。
fn rebuild_tree_preview(nodes: &[ExpandedNode], resolved_names: &[String]) -> String {
    let mut depths = vec![0usize; nodes.len()];
    let mut lines = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let depth = match node.parent_temp_id {
            Some(pid) => depths[pid] + 1,
            None => 0,
        };
        depths[i] = depth;
        let label = match &node.display_name {
            Some(d) if d != &node.name => sanitize_label(d),
            _ => sanitize_label(&resolved_names[i]),
        };
        lines.push(format!("{}{} ({})", "  ".repeat(depth), label, node.category));
    }
    lines.join("\n")
}

/// 传递加载引用：从根模板 BFS，scene_ref 子树内的 template_ref/scene_ref 一并收集。
/// visited 防环导致的无限加载（环本身由展开器检测报 400，含引用链路径）。
/// 引用不存在 → Validation（400，spec §6）；深度上限与展开器一致（MAX_SCENE_REF_DEPTH）。
async fn preload_refs(
    db: &Db,
    workspace_id: &str,
    root: &SceneTemplateFile,
) -> Result<(HashMap<String, ThingTemplate>, HashMap<String, SceneTemplateFile>)> {
    async fn load_device(
        db: &Db,
        workspace_id: &str,
        name: &str,
        device_templates: &mut HashMap<String, ThingTemplate>,
    ) -> Result<()> {
        if device_templates.contains_key(name) {
            return Ok(());
        }
        let t = db
            .find_thing_template_by_name(name, workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?
            .ok_or_else(|| MarketplaceError::Validation(format!("引用模板不存在或已停用: {}", name)))?;
        device_templates.insert(name.to_string(), t);
        Ok(())
    }

    let mut device_templates: HashMap<String, ThingTemplate> = HashMap::new();
    let mut scene_templates: HashMap<String, SceneTemplateFile> = HashMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();

    let (device_refs, scene_refs) = collect_refs(root);
    for name in device_refs {
        load_device(db, workspace_id, &name, &mut device_templates).await?;
    }
    for name in scene_refs {
        queue.push_back((name, 1));
    }

    while let Some((name, depth)) = queue.pop_front() {
        if depth >= MAX_SCENE_REF_DEPTH {
            return Err(MarketplaceError::Expand(ExpandError::TooDeep));
        }
        if !visited.insert(name.clone()) {
            continue;
        }
        let t = db
            .find_thing_template_by_name(&name, workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?
            .ok_or_else(|| MarketplaceError::Validation(format!("引用场景包不存在或已停用: {}", name)))?;
        let sub = SceneTemplateFile::from_json(&t.device_info)
            .map_err(|e| MarketplaceError::Template(format!("场景包 {} 解析失败: {}", name, e)))?;
        let (sub_devices, sub_scenes) = collect_refs(&sub);
        for d in sub_devices {
            load_device(db, workspace_id, &d, &mut device_templates).await?;
        }
        for s in sub_scenes {
            queue.push_back((s, depth + 1));
        }
        scene_templates.insert(name, sub);
    }
    Ok((device_templates, scene_templates))
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

/// 事务内落库：名称解析（与 dry-run 共用步骤）→ 拓扑序创建本体 → 子表。
/// 返回 (root_thing_id, 最终名称列表, warnings)。任何一步失败即返回 Err，由调用方回滚整个事务。
async fn persist_tree(
    db: &Db,
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    template_id: &str,
    params: &InstantiateParams,
    result: &ExpansionResult,
) -> Result<(String, Vec<String>, Vec<String>)> {
    // 事务内名称解析（与 dry-run 同算法）：事务可见自身写入，计划名即落库名（无并发干预下）
    let planned = resolve_node_names(db, Some(tx), workspace_id, &result.nodes).await?;
    let mut warnings = name_warnings(&result.nodes, &planned);
    let mut final_names: Vec<String> = Vec::with_capacity(result.nodes.len());
    let mut real_ids: HashMap<usize, String> = HashMap::new();
    let mut root_id = String::new();

    for (i, node) in result.nodes.iter().enumerate() {
        let real_parent = match node.parent_temp_id {
            Some(pid) => real_ids.get(&pid).cloned(),
            None => params.parent_id.clone(),
        };

        // 计划名先试；唯一约束冲突（TOCTOU：探测与 INSERT 之间被并发抢占）时
        // 用 resolve_thing_name_tx 重探测兜底（≤10），与改造前行为一致
        let mut tries = 0usize;
        let mut candidate = planned[i].clone();
        let (resolved, id) = loop {
            let req = build_thing_request(node, candidate.clone(), real_parent.clone(), template_id, workspace_id);
            match db.create_thing_row_with_type_tx(tx, &req, &node.thing_type).await {
                Ok(id) => break (candidate, id),
                Err(e) if is_unique_violation(&e) && tries < MAX_NAME_RETRIES => {
                    tries += 1;
                    candidate = db
                        .resolve_thing_name_tx(tx, workspace_id, &candidate)
                        .await
                        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;
                    continue;
                }
                Err(e) => return Err(MarketplaceError::Template(format!("创建本体失败: {}", e))),
            }
        };
        if resolved != planned[i] {
            warnings.push(format!("名称冲突：{} → {}", planned[i], resolved));
        }

        final_names.push(resolved);
        real_ids.insert(node.temp_id, id.clone());
        if node.temp_id == 0 {
            root_id = id.clone();
        }

        persist_children_tables(db, tx, workspace_id, &id, node, &mut warnings).await?;
    }
    Ok((root_id, final_names, warnings))
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
        name: resolved_name.clone(),
        // display_name 等于未解析名称时（默认命名模式产物）同步为解析后名称，与树上显示一致
        display_name: match &node.display_name {
            Some(d) if d == &node.name => Some(resolved_name),
            other => other.clone(),
        },
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

/// rule_type 字符串 → RuleType。展开器已校验 3 个允许值（event 不支持），此处再校验一次兜底。
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
