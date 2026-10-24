//! SceneInstantiator — 场景包实例化：展开（纯函数）→ 配额/父校验 → 名称解析 → 单事务落库。
//!
//! 数据流：
//!   template row ──▶ SceneTemplateFile::from_json(device_info)
//!        │ 收集 template_ref/scene_ref ──▶ find_thing_template_by_name (workspace→builtin)
//!        ▼
//!   expand() ──▶ ExpansionResult (nodes 拓扑序, tree_preview, warnings)
//!        │
//!        ▼
//!   extra_nodes 并入统一节点列表（模板/parent_path/count 校验 + 属性命令内联）
//!        │ driver 决策（显式 driver_name 优先，attach_simulated_driver 补 simulator）
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
use std::sync::Arc;

use sqlx::{Sqlite, Transaction};
use tinyiothub_core::models::event::{
    ContentElement, Event as DomainEvent, EventLevel, EventSource, RichContent, TextFormat, ThingEventType,
};
use tinyiothub_core::models::thing::{CreateThingRequest, Thing};
use tinyiothub_core::models::thing_command::CreateThingCommandRequest;
use tinyiothub_core::models::thing_property::CreateThingPropertyRequest;
use tinyiothub_runtime::event_bus::{EventBus, publish_event_safe};
use tinyiothub_storage::Db;
use tinyiothub_storage::alarm::AlarmLevel;
use tinyiothub_storage::alarm_rule::{AlarmCondition, AlarmRule, NotificationConfig, RuleType};
use tinyiothub_storage::scene_template::{
    ExpandError, ExpandedNode, ExpansionResult, MAX_NODES, MAX_SCENE_REF_DEPTH, SceneTemplateFile, ThingNodeDef,
    expand, localized, sanitize_label,
};
use tinyiothub_storage::thing::thing_name_candidates;
use tinyiothub_storage::thing_template::{CommandTemplate, PropertyTemplate, ThingTemplate};
use tracing::{error, info, instrument, warn};

use super::error::{MarketplaceError, Result};
use super::template_rules::resolve_template_rules;
use crate::domains::thing::hooks::{CreationMode, ThingDeclarationHook};
use crate::domains::thing::service::ThingService;

/// 唯一约束兜底重试上限（TOCTOU：SELECT 探测与 INSERT 之间被并发抢占）。
const MAX_NAME_RETRIES: usize = 10;
/// SQLite 单写者锁竞争下的整事务重试上限。
const MAX_TX_RETRIES: usize = 5;
/// extra_nodes 单条 count 上限。
const MAX_EXTRA_NODE_COUNT: u32 = 20;
/// attach_simulated_driver 填充的模拟驱动名与默认上报间隔。
const SIMULATOR_DRIVER: &str = "simulator";
const SIMULATOR_OPTIONS: &str = r#"{"interval":"3000"}"#;

/// 实例化时额外挂接的设备节点（与场景节点同一展开列表 → 同事务/同配额/同预览）。
#[derive(Debug, Clone)]
pub struct ExtraNode {
    /// 设备模板 id（必须存在且非场景包）
    pub template_id: String,
    /// 场景节点 key 路径（[] = 挂到根节点下）
    pub parent_path: Vec<String>,
    /// 节点名（count > 1 时自动追加 1..=N 序号）
    pub name: String,
    /// 份数（1..=20）
    pub count: u32,
    /// 显式驱动名；缺省时 attach_simulated_driver=true 才填 simulator
    pub driver_name: Option<String>,
}

#[derive(Debug)]
pub struct InstantiateParams {
    pub scene_name: String,
    pub parent_id: Option<String>,
    pub parameter_values: HashMap<String, i64>,
    pub dry_run: bool,
    /// true 时叶节点设备（thing_type=device 且无显式 driver）统一挂 simulator 开箱出数
    pub attach_simulated_driver: bool,
    pub extra_nodes: Vec<ExtraNode>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstantiateOutcome {
    pub node_count: usize,
    pub root_thing_id: Option<String>,
    pub tree_preview: String,
    pub warnings: Vec<String>,
    pub quota: QuotaInfo,
}

/// 配额用量（配额校验步骤已算出，透传给 dry-run 预览展示）。
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaInfo {
    /// 当前 workspace 已有本体数
    pub current: i64,
    /// 配额上限；无限制（thing_limit=0 或无订阅链）为 null
    pub limit: Option<i64>,
    /// 实例化后的预计总数（current + node_count）
    pub after: i64,
}

pub struct SceneInstantiator;

impl SceneInstantiator {
    /// 实例化（不发热加载事件）；REST 入口走 `instantiate_with_events`。
    pub async fn instantiate(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
    ) -> Result<InstantiateOutcome> {
        Self::run(db, workspace_id, template_id, params, None, None).await
    }

    /// 实例化并在提交成功后为带驱动的节点发布 DeviceCreated（metadata 带完整物）——
    /// DataServer 事件处理器据此热建驱动（与 REST 建物同一热路径，X3a），
    /// driver_name 非空的新物不重启即注册进 data_server 并出数。
    pub async fn instantiate_with_events(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
        event_bus: &Arc<EventBus>,
    ) -> Result<InstantiateOutcome> {
        Self::run(db, workspace_id, template_id, params, Some(event_bus), None).await
    }

    /// 实例化并把带驱动节点接入 E4a 声明物化：固定能力驱动跳过模板能力
    /// 拷贝；提交后对带驱动节点 `materialize`（失败记 warning，不回滚已提交
    /// 的树——与安装/升级的 drift-not-rollback 语义一致）。
    pub async fn instantiate_with_declarations(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
        event_bus: Option<&Arc<EventBus>>,
        declaration_hook: Option<&dyn ThingDeclarationHook>,
    ) -> Result<InstantiateOutcome> {
        Self::run(db, workspace_id, template_id, params, event_bus, declaration_hook).await
    }

    #[instrument(skip(db, params, event_bus, declaration_hook), fields(dry_run = params.dry_run))]
    async fn run(
        db: &Db,
        workspace_id: &str,
        template_id: &str,
        params: &InstantiateParams,
        event_bus: Option<&Arc<EventBus>>,
        declaration_hook: Option<&dyn ThingDeclarationHook>,
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
        let outcome = Self::do_instantiate(db, workspace_id, template_id, params, event_bus, declaration_hook).await;
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
        event_bus: Option<&Arc<EventBus>>,
        declaration_hook: Option<&dyn ThingDeclarationHook>,
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
        let mut result = expand(
            &scene,
            &params.scene_name,
            &params.parameter_values,
            &device_templates,
            &scene_templates,
        )?;

        // 3.5 extra_nodes 校验并并入统一节点列表（场景节点+挂接设备同列表 →
        // 同事务落库、同配额口径、dry-run 预览同树）；driver 决策在合并后统一计算
        let explicit_drivers = merge_extra_nodes(db, workspace_id, &params.extra_nodes, &mut result).await?;
        let drivers = resolve_node_drivers(&result.nodes, &explicit_drivers, params.attach_simulated_driver);

        // 3.6 E4a：预判每个驱动节点的创建模式。固定能力驱动（Declarative）
        // 跳过模板能力拷贝；声明不可用只记 warning（保持旧场景对未安装
        // 驱动的兼容，不整包失败）。
        let mut skip_caps = vec![false; result.nodes.len()];
        let mut declaration_warnings: Vec<String> = Vec::new();
        if let Some(hook) = declaration_hook {
            for (i, driver) in drivers.iter().enumerate() {
                let Some(driver) = driver else { continue };
                match hook
                    .prepare_creation(workspace_id, &driver.name, driver.options.as_deref())
                    .await
                {
                    Ok(CreationMode::Declarative) => skip_caps[i] = true,
                    Ok(_) => {}
                    Err(e) => {
                        declaration_warnings.push(format!("驱动 '{}' 声明不可用（{e}），保留模板能力", driver.name))
                    }
                }
            }
        }

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
        // 透传配额用量（dry-run 预览展示；i64::MAX 为无限制哨兵，对外返回 null）
        let quota = QuotaInfo {
            current,
            limit: if limit == i64::MAX { None } else { Some(limit) },
            after: current + result.node_count as i64,
        };

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
            warnings.extend(declaration_warnings);
            let tree_preview = rebuild_tree_preview(&result.nodes, &resolved);
            return Ok(InstantiateOutcome {
                node_count: result.node_count,
                root_thing_id: None,
                tree_preview,
                warnings,
                quota,
            });
        }

        // 7. 单事务落库；SQLite 单写者锁竞争时整体回滚重试
        let mut attempt = 0usize;
        let mut persisted = loop {
            let mut tx = db.pool().begin().await.map_err(|e| db_err("开启事务", e))?;
            match persist_tree(
                db,
                &mut tx,
                workspace_id,
                template_id,
                params,
                &result,
                &drivers,
                &skip_caps,
                &scene,
            )
            .await
            {
                Ok(mut persisted) => {
                    tx.commit().await.map_err(|e| db_err("提交事务", e))?;
                    persisted.warnings.extend(result.warnings.clone());
                    persisted.warnings.extend(declaration_warnings.clone());
                    break persisted;
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

        // 8a. E4a：提交后物化带驱动节点的声明。失败记 warning，不回滚已提交
        // 的树（与安装/升级的 drift-not-rollback 语义一致）。
        if let Some(hook) = declaration_hook {
            for thing_id in &persisted.driver_thing_ids {
                if let Err(e) = hook.materialize(workspace_id, thing_id).await {
                    persisted
                        .warnings
                        .push(format!("物 {thing_id} 声明物化失败（模板能力已保留）: {e}"));
                }
            }
        }

        // 8b. 驱动热加载（X3a）：提交成功后为带驱动的节点发布 DeviceCreated，
        // DataServer 事件处理器热建驱动——新物不重启即出数。best-effort：
        // 发布失败只记 warn，不影响已提交的实例化结果。
        if let Some(bus) = event_bus {
            publish_driver_things_created(db, workspace_id, &persisted.driver_thing_ids, bus).await;
        }

        Ok(InstantiateOutcome {
            node_count: result.node_count,
            root_thing_id: Some(persisted.root_id),
            tree_preview: rebuild_tree_preview(&result.nodes, &persisted.final_names),
            warnings: persisted.warnings,
            quota,
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
        MarketplaceError::LockContention(_) => "lock_contention",
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
            .map_err(|e| db_err("名称探测", e))?;
            if !exists {
                picked = Some(candidate);
                break;
            }
        }
        let name = picked
            .ok_or_else(|| MarketplaceError::Validation(format!("同名冲突过多（{}），请手动指定名称", node.name)))?;
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

// ──────────────────────────────────────────────
// extra_nodes：挂接设备并入统一节点列表（X3a）
// ──────────────────────────────────────────────

/// 落库 driver 决策结果（仅 thing_type=device 的节点可能有值）。
#[derive(Debug, Clone)]
struct NodeDriver {
    name: String,
    options: Option<String>,
}

/// extra_nodes 校验并并入展开结果：模板存在且非场景包、parent_path 命中场景节点 key、
/// count ≤20；属性/命令/事件内联（属性/命令解析对齐 Fix A：列 JSON 优先 + device_info
/// 场景段回退 + snake_case/camelCase 键兼容）。dry-run 与 commit 同路径（只读校验）。
/// 返回 temp_id → 显式 driver_name 映射（attach_simulated_driver 决策的输入）。
async fn merge_extra_nodes(
    db: &Db,
    workspace_id: &str,
    extras: &[ExtraNode],
    result: &mut ExpansionResult,
) -> Result<HashMap<usize, String>> {
    let mut explicit_drivers: HashMap<usize, String> = HashMap::new();
    for (n, extra) in extras.iter().enumerate() {
        validate_extra_node(n, extra)?;
        let tpl = db
            .find_thing_template_by_id(&extra.template_id, workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?
            .ok_or_else(|| {
                MarketplaceError::Validation(format!("extra_nodes[{n}] 模板不存在: {}", extra.template_id))
            })?;
        // 服务端兜底：场景包不能作为挂接设备（前端已过滤，API 直调必须拦住）
        if tpl.category == "scenes" || tpl.is_composition() {
            return Err(MarketplaceError::Validation(format!(
                "extra_nodes[{n}] 模板 {} 是场景包（category=scenes），请改用设备模板",
                extra.template_id
            )));
        }
        let parent_temp_id =
            resolve_parent_path(&result.nodes, &extra.parent_path, &mut result.warnings).ok_or_else(|| {
                MarketplaceError::Validation(format!(
                    "extra_nodes[{n}] 的 parent_path [{}] 未命中任何场景节点 key",
                    extra.parent_path.join("/")
                ))
            })?;

        // 属性/命令内联（Fix A 路径）；事件与展开器 template_ref 分支同口径内联
        let props_json = ThingService::nonempty_json(&Some(tpl.properties.clone()))
            .or_else(|| ThingService::scene_section(&Some(tpl.device_info.clone()), "properties"));
        let properties = parse_extra_properties(props_json.as_deref(), &extra.template_id, &mut result.warnings);
        let acts_json = ThingService::nonempty_json(&Some(tpl.actions.clone()))
            .or_else(|| ThingService::scene_section(&Some(tpl.device_info.clone()), "commands"));
        let commands = parse_extra_commands(acts_json.as_deref(), &extra.template_id, &mut result.warnings);
        let events: Vec<serde_json::Value> = match serde_json::from_str(&tpl.events_json_for_inline()) {
            Ok(v) => v,
            Err(e) => {
                result
                    .warnings
                    .push(format!("模板 {} 的 events 解析失败（{e}），已跳过", extra.template_id));
                Vec::new()
            }
        };

        let base_name = extra.name.trim();
        for i in 1..=extra.count {
            if result.nodes.len() >= MAX_NODES {
                return Err(MarketplaceError::Validation(format!(
                    "展开节点数超过上限 {MAX_NODES}（含 extra_nodes）"
                )));
            }
            let temp_id = result.nodes.len();
            // count > 1 时序号后缀保证基本可区分；残留冲突由名称解析加 -N 兜底
            let name = if extra.count == 1 {
                base_name.to_string()
            } else {
                format!("{base_name}{i}")
            };
            result.nodes.push(ExpandedNode {
                temp_id,
                parent_temp_id: Some(parent_temp_id),
                key: None,
                display_name: Some(name.clone()),
                name,
                category: tpl.category.clone(),
                thing_type: "device".to_string(),
                properties: properties.clone(),
                commands: commands.clone(),
                event_defs: events.clone(),
                knowledge: None,
                resources: Vec::new(),
                dashboard: None,
                alarm_rules: Vec::new(),
            });
            if let Some(d) = &extra.driver_name {
                explicit_drivers.insert(temp_id, d.clone());
            }
        }
    }
    // node_count 并入统一口径：配额校验与响应计数都含 extra_nodes
    result.node_count = result.nodes.len();
    Ok(explicit_drivers)
}

/// extra_nodes 单条校验：name 合法（与 scene_name 同规则）+ count ∈ 1..=20。
fn validate_extra_node(index: usize, extra: &ExtraNode) -> Result<()> {
    validate_scene_name(&extra.name).map_err(|e| match e {
        MarketplaceError::Validation(msg) => {
            MarketplaceError::Validation(format!("extra_nodes[{index}] name 非法: {msg}"))
        }
        other => other,
    })?;
    if extra.count == 0 || extra.count > MAX_EXTRA_NODE_COUNT {
        return Err(MarketplaceError::Validation(format!(
            "extra_nodes[{index}] count 须在 1..={MAX_EXTRA_NODE_COUNT} 之间（实际 {}）",
            extra.count
        )));
    }
    Ok(())
}

/// parent_path 解析：[] = 根节点；逐段在当前节点子级中按场景节点 key 匹配，
/// key 重名（count 多份展开）取拓扑序首个并记 warning。未命中返回 None。
fn resolve_parent_path(nodes: &[ExpandedNode], path: &[String], warnings: &mut Vec<String>) -> Option<usize> {
    let mut current = nodes.iter().find(|n| n.parent_temp_id.is_none())?.temp_id;
    for seg in path {
        let mut matches = nodes
            .iter()
            .filter(|n| n.parent_temp_id == Some(current) && n.key.as_deref() == Some(seg.as_str()));
        let first = matches.next()?;
        if matches.next().is_some() {
            warnings.push(format!("parent_path 段「{seg}」命中多个场景节点，已取首个"));
        }
        current = first.temp_id;
    }
    Some(current)
}

/// driver 决策：显式 driver_name（extra_nodes 声明）优先；其次 attach_simulated_driver
/// 对叶设备（thing_type=device 且无显式 driver）统一填 simulator 开箱出数；
/// 空间/楼栋等非设备节点恒无 driver。
fn resolve_node_drivers(
    nodes: &[ExpandedNode],
    explicit: &HashMap<usize, String>,
    attach_simulated: bool,
) -> Vec<Option<NodeDriver>> {
    nodes
        .iter()
        .map(|n| {
            if n.thing_type != "device" {
                return None;
            }
            if let Some(name) = explicit.get(&n.temp_id) {
                return Some(NodeDriver {
                    name: name.clone(),
                    options: None,
                });
            }
            attach_simulated.then(|| NodeDriver {
                name: SIMULATOR_DRIVER.to_string(),
                options: Some(SIMULATOR_OPTIONS.to_string()),
            })
        })
        .collect()
}

/// i18n 字段归一化为多语言 map：对象原样保留字符串值，裸字符串按 zh 收纳。
/// Fix A i18n_pick 的形状适配——实例化器走类型化 PropertyTemplate（persist 路径复用），
/// 而非 Fix A 的拍平直接写行；取值语义一致（zh > en 由下游 localized 回退承担）。
fn i18n_map(v: Option<&serde_json::Value>) -> HashMap<String, String> {
    match v {
        Some(serde_json::Value::Object(o)) => o
            .iter()
            .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        Some(serde_json::Value::String(s)) => HashMap::from([("zh".to_string(), s.clone())]),
        _ => HashMap::new(),
    }
}

fn i18n_map_opt(v: Option<&serde_json::Value>) -> Option<HashMap<String, String>> {
    let m = i18n_map(v);
    if m.is_empty() { None } else { Some(m) }
}

/// 模板属性 JSON → PropertyTemplate（Fix A 键兼容：snake_case 优先、camelCase 兜底）。
/// 解析失败记 warning 后按空块继续——与展开器 template_ref 分支同策略。
fn parse_extra_properties(json: Option<&str>, template_id: &str, warnings: &mut Vec<String>) -> Vec<PropertyTemplate> {
    let Some(json) = json else {
        return Vec::new();
    };
    let values: Vec<serde_json::Value> = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!("模板 {template_id} 的 properties 解析失败（{e}），已跳过"));
            return Vec::new();
        }
    };
    values
        .iter()
        .map(|p| PropertyTemplate {
            name: p["name"].as_str().unwrap_or_default().to_string(),
            display_name: i18n_map(ThingService::key(p, "display_name", "displayName")),
            description: i18n_map_opt(p.get("description")),
            data_type: ThingService::key(p, "data_type", "dataType")
                .and_then(|v| v.as_str())
                .unwrap_or("string")
                .to_string(),
            unit: p.get("unit").and_then(|v| v.as_str()).map(str::to_string),
            min_value: ThingService::key(p, "min_value", "minValue").and_then(|v| v.as_f64()),
            max_value: ThingService::key(p, "max_value", "maxValue").and_then(|v| v.as_f64()),
            default_value: ThingService::key(p, "default_value", "defaultValue").map(|v| match v.as_str() {
                Some(s) => s.to_string(),
                None => v.to_string(),
            }),
            is_read_only: ThingService::key(p, "is_read_only", "isReadOnly")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            is_required: ThingService::key(p, "is_required", "isRequired")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            validation_rules: None,
        })
        .collect()
}

/// 模板命令 JSON → CommandTemplate（Fix A 键兼容 + parameters 原样 JSON 字符串）。
fn parse_extra_commands(json: Option<&str>, template_id: &str, warnings: &mut Vec<String>) -> Vec<CommandTemplate> {
    let Some(json) = json else {
        return Vec::new();
    };
    let values: Vec<serde_json::Value> = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!("模板 {template_id} 的 actions 解析失败（{e}），已跳过"));
            return Vec::new();
        }
    };
    values
        .iter()
        .map(|a| CommandTemplate {
            name: a["name"].as_str().unwrap_or_default().to_string(),
            display_name: i18n_map(ThingService::key(a, "display_name", "displayName")),
            description: i18n_map_opt(a.get("description")),
            parameters: a.get("parameters").map(|v| v.to_string()),
            parameter_schema: None,
            is_required: false,
        })
        .collect()
}

/// 落库产物：根 id、最终名称、warnings，以及带驱动节点的真实 thing id
/// （提交后热加载事件的输入——DataServer 按 DeviceCreated 热建驱动）。
struct PersistedTree {
    root_id: String,
    final_names: Vec<String>,
    warnings: Vec<String>,
    driver_thing_ids: Vec<String>,
}

/// 事务内落库：名称解析（与 dry-run 共用步骤）→ 拓扑序创建本体 → 子表。
/// 任何一步失败即返回 Err，由调用方回滚整个事务。
#[allow(clippy::too_many_arguments)]
async fn persist_tree(
    db: &Db,
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    template_id: &str,
    params: &InstantiateParams,
    result: &ExpansionResult,
    drivers: &[Option<NodeDriver>],
    skip_caps: &[bool],
    scene: &SceneTemplateFile,
) -> Result<PersistedTree> {
    // 事务内名称解析（与 dry-run 同算法）：事务可见自身写入，计划名即落库名（无并发干预下）
    let planned = resolve_node_names(db, Some(tx), workspace_id, &result.nodes).await?;
    let mut warnings = name_warnings(&result.nodes, &planned);
    let mut final_names: Vec<String> = Vec::with_capacity(result.nodes.len());
    let mut real_ids: HashMap<usize, String> = HashMap::new();
    let mut driver_thing_ids: Vec<String> = Vec::new();
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
            let req = build_thing_request(
                node,
                candidate.clone(),
                real_parent.clone(),
                template_id,
                workspace_id,
                &drivers[i],
            );
            match insert_thing_node_tx(db, tx, &req, &node.thing_type).await {
                Ok(id) => break (candidate, id),
                Err(e) if is_name_unique_violation(&e) => {
                    if tries >= MAX_NAME_RETRIES {
                        // 重试耗尽：归位 400（用户可手动指定名称重试），非内部错误
                        return Err(MarketplaceError::Validation(format!(
                            "同名冲突过多（{}），请手动指定名称",
                            node.name
                        )));
                    }
                    tries += 1;
                    candidate = db
                        .resolve_thing_name_tx(tx, workspace_id, &candidate)
                        .await
                        .map_err(|e| match e {
                            // 只有"同名冲突过多"（Protocol 哨兵）映射 400；其余 DB 错误保持 500
                            sqlx::Error::Protocol(msg) => MarketplaceError::Validation(msg),
                            other => db_err("名称重探测", other),
                        })?;
                    continue;
                }
                Err(e) => return Err(db_err("创建本体失败", e)),
            }
        };
        if resolved != planned[i] {
            warnings.push(format!("名称冲突：{} → {}", planned[i], resolved));
        }

        final_names.push(resolved);
        real_ids.insert(node.temp_id, id.clone());
        if drivers[i].is_some() {
            driver_thing_ids.push(id.clone());
        }
        if node.temp_id == 0 {
            root_id = id.clone();
        }

        persist_children_tables(
            db,
            tx,
            workspace_id,
            &id,
            node,
            skip_caps.get(i).copied().unwrap_or(false),
            &mut warnings,
        )
        .await?;
    }

    // E3：场景包自带的 watch_rules/composites 同事务落地（"装完即自带值守"）。
    apply_scene_rules_tx(tx, workspace_id, result, &real_ids, scene).await?;

    Ok(PersistedTree {
        root_id,
        final_names,
        warnings,
        driver_thing_ids,
    })
}

/// E3：场景包自带 watch_rules/composites 的同事务落地。
/// 引用映射 = 节点 key → 新建 thing_id；任一引用/冲突失败 → 整包回滚。
async fn apply_scene_rules_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    result: &ExpansionResult,
    real_ids: &HashMap<usize, String>,
    scene: &SceneTemplateFile,
) -> Result<()> {
    if scene.watch_rules.is_empty() && scene.composites.is_empty() {
        return Ok(());
    }
    let mut mapping: HashMap<String, String> = HashMap::new();
    for node in &result.nodes {
        if let Some(key) = &node.key
            && let Some(id) = real_ids.get(&node.temp_id)
        {
            mapping.entry(key.clone()).or_insert_with(|| id.clone());
        }
    }
    let template = serde_json::json!({
        "watch_rules": scene.watch_rules,
        "composites": scene.composites,
    });
    let existing = tinyiothub_storage::composite::list_composite_names_tx(tx, workspace_id)
        .await
        .map_err(|e| db_err("读取 composite 名称", e))?;
    let (resolved, report) = resolve_template_rules(&template, workspace_id, &mapping, &existing);
    if !report.ok {
        let detail = report
            .references
            .iter()
            .map(|r| format!("{}:{}", r.path, r.status))
            .chain(report.conflicts.iter().map(|c| format!("{}:{}", c.kind, c.ref_id)))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(MarketplaceError::Validation(format!(
            "场景包值守规则校验失败（整包回滚）：{detail}"
        )));
    }
    for rule in &resolved.watch_rules {
        let condition = serde_json::to_value(&rule.condition)
            .map_err(|e| MarketplaceError::Template(format!("watch 条件序列化失败: {e}")))?;
        let kind = condition.get("type").and_then(|t| t.as_str()).unwrap_or("property");
        tinyiothub_storage::watch::insert_rule_tx(
            tx,
            &tinyiothub_storage::watch::WatchRuleRow {
                id: rule.id.clone(),
                workspace_id: rule.workspace_id.clone(),
                kind: kind.to_string(),
                condition: condition.to_string(),
                persistent: i64::from(rule.persistent),
                timeout_secs: i64::from(rule.timeout_secs),
                created_at: rule.created_at.clone(),
            },
        )
        .await
        .map_err(|e| db_err("watch 规则落库", e))?;
    }
    for def in &resolved.composites {
        let definition =
            serde_json::to_string(def).map_err(|e| MarketplaceError::Template(format!("composite 序列化失败: {e}")))?;
        tinyiothub_storage::composite::insert_composite_tx(
            tx,
            &tinyiothub_storage::composite::CompositeRow {
                id: def.id.clone(),
                workspace_id: def.workspace_id.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                definition,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .map_err(|e| db_err("composite 落库", e))?;
    }
    Ok(())
}

/// 名称插入（persist_tree 唯一调用点）。生产路径直通
/// `Db::create_thing_row_with_type_tx`；`#[cfg(test)]` 下提供故障注入 seam，
/// 用于覆盖 SQLite 下跨连接不可达的 TOCTOU 唯一约束重探测路径。
async fn insert_thing_node_tx(
    db: &Db,
    tx: &mut Transaction<'_, Sqlite>,
    req: &CreateThingRequest,
    thing_type: &str,
) -> std::result::Result<String, sqlx::Error> {
    #[cfg(test)]
    toctou_fault::maybe_preempt_name(tx, req, thing_type).await?;
    db.create_thing_row_with_type_tx(tx, req, thing_type).await
}

/// things 名称索引唯一冲突判定：只对 idx_things_name_workspace 的冲突进重试。
/// 其他唯一索引（如 thing_properties UNIQUE(thing_id,name)）的冲突不属名称竞态，
/// 直接冒泡为 500，不进重探测循环。
fn is_name_unique_violation(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(dbe) if dbe.is_unique_violation() => {
            let msg = dbe.message();
            msg.contains("idx_things_name_workspace") || msg.contains("things.name")
        }
        _ => false,
    }
}

/// SQLite 锁竞争（BUSY/LOCKED）判定：匹配扩展错误码主码（SQLITE_BUSY=5 /
/// SQLITE_LOCKED=6，含 BUSY_SNAPSHOT=517、LOCKED_SHAREDCACHE=262 等扩展码），
/// 不做错误文本子串匹配。
fn is_sqlite_lock_contention(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(dbe) => dbe
            .code()
            .and_then(|c| c.parse::<i64>().ok())
            .is_some_and(|code| matches!(code & 0xFF, 5 | 6)),
        _ => false,
    }
}

/// sqlx 错误归位：锁竞争 → LockContention（整事务重试信号）；其余 → Template(500)。
fn db_err(context: &str, e: sqlx::Error) -> MarketplaceError {
    if is_sqlite_lock_contention(&e) {
        MarketplaceError::LockContention(format!("{context}: {e}"))
    } else {
        MarketplaceError::Template(format!("{context}: {e}"))
    }
}

/// DbError 归位：解包 Sqlx 后按 db_err 同口径分类。
fn storage_err(context: &str, e: tinyiothub_storage::DbError) -> MarketplaceError {
    match e {
        tinyiothub_storage::DbError::Sqlx(e) => db_err(context, e),
        other => MarketplaceError::Template(format!("{context}: {other}")),
    }
}

/// 整事务回滚重试的信号：仅 LockContention 变体（构造点已按扩展错误码判定）。
fn is_lock_contention(e: &MarketplaceError) -> bool {
    matches!(e, MarketplaceError::LockContention(_))
}

fn build_thing_request(
    node: &ExpandedNode,
    resolved_name: String,
    parent_id: Option<String>,
    template_id: &str,
    workspace_id: &str,
    driver: &Option<NodeDriver>,
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
        driver_name: driver.as_ref().map(|d| d.name.clone()),
        device_model: None,
        protocol_type: None,
        factory_name: None,
        linked_data,
        driver_options: driver.as_ref().and_then(|d| d.options.clone()),
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
    skip_capabilities: bool,
    warnings: &mut Vec<String>,
) -> Result<()> {
    // 固定能力驱动（Declarative）：能力来自 manifest，模板属性/命令不得污染。
    if !skip_capabilities {
        // 属性（display_name 多语言回退 zh → en → name，spec §2.4）
        let props: Vec<CreateThingPropertyRequest> = node
            .properties
            .iter()
            .map(|p| CreateThingPropertyRequest {
                thing_id: thing_id.to_string(),
                name: p.name.clone(),
                display_name: localized(&p.display_name).or_else(|| Some(p.name.clone())),
                description: p.description.as_ref().and_then(localized),
                data_type: Some(p.data_type.clone()),
                unit: p.unit.clone(),
                min_value: p.min_value,
                max_value: p.max_value,
                default_value: p.default_value.clone(),
                is_read_only: Some(p.is_read_only as i32),
                expression: None,
                source: None,
                refs: None,
            })
            .collect();
        db.create_thing_properties_batch_tx(tx, &props)
            .await
            .map_err(|e| db_err("创建属性失败", e))?;

        // 命令（display_name 回退同属性）
        let cmds: Vec<CreateThingCommandRequest> = node
            .commands
            .iter()
            .map(|c| CreateThingCommandRequest {
                thing_id: thing_id.to_string(),
                name: c.name.clone(),
                display_name: localized(&c.display_name).or_else(|| Some(c.name.clone())),
                description: c.description.as_ref().and_then(localized),
                parameters: c.parameters.clone(),
            })
            .collect();
        db.bulk_create_thing_commands_tx(tx, &cmds)
            .await
            .map_err(|e| db_err("创建命令失败", e))?;
    }

    // 资源（file_path = uri 原样记录，v1 无真实托管）
    for r in &node.resources {
        db.insert_thing_resource_tx(tx, workspace_id, thing_id, &r.resource_type, &r.name, &r.uri)
            .await
            .map_err(|e| db_err("创建资源失败", e))?;
    }

    // 告警规则：property_ref → 本节点真实 property_id
    for rule in &node.alarm_rules {
        if skip_capabilities && rule.property_ref.is_some() {
            warnings.push(format!(
                "告警规则 {} 引用的模板属性未拷贝（声明式驱动），已跳过",
                rule.name
            ));
            continue;
        }
        let property_id = match &rule.property_ref {
            Some(ref_name) => {
                let found = db
                    .find_thing_property_id_by_name_tx(tx, thing_id, ref_name)
                    .await
                    .map_err(|e| db_err("查询属性失败", e))?;
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
        let alarm_level = match parse_alarm_level(&rule.alarm_level) {
            Some(level) => level,
            None => {
                // 未知 level 不静默降级：记 warning 后按 warning 处理（warnings 非阻断策略）
                warnings.push(format!(
                    "告警规则 {} 的 alarm_level「{}」未知，已按 warning 处理",
                    rule.name, rule.alarm_level
                ));
                AlarmLevel::Warning
            }
        };
        // notification_config：缺失（null）/空对象 {} 按 spec 缺省（Default）；
        // 非空对象解析失败记 warning 后按缺省（不静默吞掉配置错误）
        let notification_config = match &rule.notification_config {
            serde_json::Value::Null => NotificationConfig::default(),
            v if v.as_object().is_some_and(|o| o.is_empty()) => NotificationConfig::default(),
            v => match serde_json::from_value::<NotificationConfig>(v.clone()) {
                Ok(c) => c,
                Err(e) => {
                    warnings.push(format!(
                        "告警规则 {} 的 notification_config 解析失败（{}），已按缺省处理",
                        rule.name, e
                    ));
                    NotificationConfig::default()
                }
            },
        };
        let alarm = AlarmRule::new(
            rule.name.clone(),
            rule.description.clone(),
            Some(thing_id.to_string()),
            property_id,
            parse_rule_type(&rule.rule_type)?,
            serde_json::from_value::<AlarmCondition>(rule.condition.clone())
                .map_err(|e| MarketplaceError::Validation(format!("告警条件格式错误: {}", e)))?,
            alarm_level,
            notification_config,
            workspace_id.to_string(),
        )
        .map_err(|e| MarketplaceError::Validation(e.to_string()))?;
        db.create_alarm_rule_tx(tx, &alarm)
            .await
            .map_err(|e| storage_err("创建告警规则失败", e))?;
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

/// alarm_level 字符串 → AlarmLevel；未知值返回 None，由调用方记 warning 后降级。
fn parse_alarm_level(s: &str) -> Option<AlarmLevel> {
    match s {
        "info" => Some(AlarmLevel::Info),
        "warning" => Some(AlarmLevel::Warning),
        "error" => Some(AlarmLevel::Error),
        "critical" => Some(AlarmLevel::Critical),
        _ => None,
    }
}

/// 提交成功后为带驱动的节点发布 DeviceCreated（metadata 带完整物，与 REST 建物
/// 同形状）——DataServer 事件处理器据此热建驱动并插入 device_cache（复用既有
/// 热路径，不重写注册逻辑）。best-effort：加载/构造失败只记 warn，不影响已提交
/// 的实例化结果（与 REST 路径 `let _ =` 同语义）。
async fn publish_driver_things_created(db: &Db, workspace_id: &str, thing_ids: &[String], event_bus: &Arc<EventBus>) {
    for id in thing_ids {
        match load_complete_thing(db, workspace_id, id).await {
            Ok(Some(device)) => {
                let device_json = serde_json::to_value(&device).unwrap_or(serde_json::Value::Null);
                let content = RichContent::new(
                    format!("Thing Created: {}", device.name),
                    vec![ContentElement::Text {
                        content: format!("Thing '{}' has been created successfully", device.name),
                        format: TextFormat::Plain,
                    }],
                )
                .with_metadata("device".to_string(), device_json);
                match DomainEvent::new_device_event(
                    ThingEventType::DeviceCreated,
                    EventLevel::Info,
                    EventSource::device(device.id.clone(), Some("scene_instantiator".to_string())),
                    content,
                ) {
                    Ok(event) => publish_event_safe(event_bus.clone(), event).await,
                    Err(e) => warn!(thing_id = %id, error = %e, "实例化热加载：DeviceCreated 事件构造失败"),
                }
            }
            Ok(None) => warn!(thing_id = %id, "实例化热加载：提交后读不到新物"),
            Err(e) => warn!(thing_id = %id, error = %e, "实例化热加载：加载完整物失败"),
        }
    }
}

/// 加载完整物（含属性——模拟驱动按属性产值；DataServer 的 device_cache 插入同用）。
async fn load_complete_thing(db: &Db, workspace_id: &str, thing_id: &str) -> Result<Option<Thing>> {
    let mut thing = db
        .find_thing_by_id(Some(workspace_id), thing_id)
        .await
        .map_err(|e| MarketplaceError::Template(format!("加载实例化物失败: {e}")))?;
    if let Some(t) = &mut thing {
        t.properties = Some(
            db.find_thing_properties_by_thing_id(thing_id)
                .await
                .map_err(|e| MarketplaceError::Template(format!("加载实例化物属性失败: {e}")))?,
        );
    }
    Ok(thing)
}

/// 测试故障注入（不进生产二进制）：arming 后下一次名称插入先在本事务内
/// 抢占计划名，使随后的 INSERT 抛真实唯一约束冲突，驱动调用方的
/// `is_unique_violation` 重探测重试路径。thread_local 保证并发测试间互不干扰
/// （`#[tokio::test]` 默认 current_thread runtime，测试任务不跨线程迁移）。
/// 置于文件末尾：clippy `items_after_test_module` 要求 cfg(test) mod 之后无其他项。
#[cfg(test)]
pub(crate) mod toctou_fault {
    use std::cell::Cell;

    use sqlx::{Sqlite, Transaction};
    use tinyiothub_core::models::thing::CreateThingRequest;

    thread_local! {
        static ARMED: Cell<bool> = const { Cell::new(false) };
    }

    /// 武装一次故障：本线程下一次 `insert_thing_node_tx` 前抢占 `req.name`。
    pub(crate) fn arm_once() {
        ARMED.with(|c| c.set(true));
    }

    pub(super) async fn maybe_preempt_name(
        tx: &mut Transaction<'_, Sqlite>,
        req: &CreateThingRequest,
        thing_type: &str,
    ) -> Result<(), sqlx::Error> {
        if !ARMED.with(|c| c.replace(false)) {
            return Ok(());
        }
        sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('toctou-fault-1', ?, ?, ?)")
            .bind(&req.name)
            .bind(req.workspace_id.as_deref().unwrap_or_default())
            .bind(thing_type)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }
}
