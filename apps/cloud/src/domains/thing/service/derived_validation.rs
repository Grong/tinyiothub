//! 派生表达式保存时校验（spec §二）：语法 → 上限 → refs 解析（物名→id，同 workspace）
//! → DAG 环检测。保存入口（Task 8）的唯一校验点。

use sqlx::SqlitePool;
use tinyiothub_runtime::expression;

#[derive(Debug)]
pub struct ResolvedRef {
    pub thing_id: String,
    pub property_name: String,
}

pub async fn validate_derived_expression(
    pool: &SqlitePool,
    workspace_id: &str,
    owner_thing_id: &str,
    expr: &str,
) -> Result<Vec<ResolvedRef>, String> {
    let ast = expression::parse(expr)?; // 语法 + F-C 上限
    let refs = expression::extract_refs(&ast);
    let mut resolved = Vec::with_capacity(refs.len());
    let mut invalid = Vec::new();
    for r in &refs {
        // 物名 → id（workspace 作用域；表名 things，20260825000001 改名）；属性须存在于该物
        let thing = sqlx::query_as::<_, (String,)>("SELECT id FROM things WHERE workspace_id = ? AND name = ?")
            .bind(workspace_id)
            .bind(&r.thing_name)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;
        match thing {
            Some((id,)) => {
                let exists: Option<(String,)> =
                    sqlx::query_as("SELECT id FROM thing_properties WHERE thing_id = ? AND name = ?")
                        .bind(&id)
                        .bind(&r.property_name)
                        .fetch_optional(pool)
                        .await
                        .map_err(|e| e.to_string())?;
                if exists.is_none() {
                    invalid.push(format!("{} 无属性 {}", r.thing_name, r.property_name));
                } else {
                    resolved.push(ResolvedRef {
                        thing_id: id,
                        property_name: r.property_name.clone(),
                    });
                }
            }
            None => invalid.push(format!("物 {} 不存在", r.thing_name)),
        }
    }
    if !invalid.is_empty() {
        return Err(format!("无效引用: {}", invalid.join("; ")));
    }
    // DAG：owner → 每个引用物一条边；与既有派生边合并后从 owner DFS 找环
    detect_cycle(pool, workspace_id, owner_thing_id, &resolved).await?;
    Ok(resolved)
}

async fn detect_cycle(
    pool: &SqlitePool,
    workspace_id: &str,
    owner_thing_id: &str,
    new_edges: &[ResolvedRef],
) -> Result<(), String> {
    // 既有边：workspace 内所有派生属性 thing_id -> refs.thing_id（refs 为 JSON）
    // COALESCE 双保险（Task 6 评审）：历史行 refs 可能为 NULL，按 '[]' 处理不炸行。
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"SELECT p.thing_id, COALESCE(p.refs, '[]') FROM thing_properties p
           JOIN things d ON d.id = p.thing_id
           WHERE d.workspace_id = ? AND p.expression IS NOT NULL"#,
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut graph: std::collections::HashMap<String, Vec<String>> = Default::default();
    for (owner, refs_json) in rows {
        let ids: Vec<serde_json::Value> = serde_json::from_str(&refs_json).unwrap_or_default();
        for v in ids {
            if let Some(t) = v.get("thing_id").and_then(|x| x.as_str()) {
                graph.entry(owner.clone()).or_default().push(t.to_string());
            }
        }
    }
    // 新边（同 owner 多条属性时既有边已含旧引用；此处校验的是"新增后是否成环"，
    // 保守做法：新边全量加入——同 key 的旧边由调用方在更新路径先移除，见 Task 8 PUT 语义）
    for e in new_edges {
        graph
            .entry(owner_thing_id.to_string())
            .or_default()
            .push(e.thing_id.clone());
    }
    // DFS 从 owner 出发能回到 owner 即环（含自环）
    let mut stack = vec![owner_thing_id.to_string()];
    let mut seen = std::collections::HashSet::new();
    while let Some(n) = stack.pop() {
        for next in graph.get(&n).cloned().unwrap_or_default() {
            if next == owner_thing_id {
                return Err(format!("引用成环: {} → … → {}", owner_thing_id, owner_thing_id));
            }
            if seen.insert(next.clone()) {
                stack.push(next);
            }
        }
    }
    Ok(())
}
