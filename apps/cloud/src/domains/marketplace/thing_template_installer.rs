// Thing Template Installer — local marketplace category for thing_templates.
// Lists and installs (copies) thing_templates from the local database,
// supporting workspace-scoped name conflict resolution.
//
// SQL 已迁入 tinyiothub_storage::thing_template（Task 12）；本文件保留
// marketplace 特有的响应映射与名称冲突解决逻辑。

use tinyiothub_storage::Db;

use super::error::{MarketplaceError, Result};

// ──────────────────────────────────────────────
// Response types
// ──────────────────────────────────────────────

/// Lightweight thing_template item for marketplace listing.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThingTemplateItem {
    pub id: String,
    pub name: String,
    pub thing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub property_count: usize,
    pub action_count: usize,
    pub event_count: usize,
    pub is_builtin: bool,
    pub category: String,
    pub created_at: String,
}

/// Installed template result.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledTemplate {
    pub id: String,
    pub name: String,
}

// ──────────────────────────────────────────────
// Installer
// ──────────────────────────────────────────────

pub struct ThingTemplateInstaller;

impl ThingTemplateInstaller {
    /// List thing_templates available as marketplace items.
    /// Shows built-in templates (workspace_id IS NULL) first,
    /// then workspace-scoped templates.
    pub async fn list(db: &Db, workspace_id: &str) -> Result<Vec<ThingTemplateItem>> {
        let rows = db
            .list_marketplace_thing_templates(workspace_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                let property_count = json_array_len(&r.properties);
                let action_count = json_array_len(&r.actions);
                let event_count = json_array_len(&r.events);
                ThingTemplateItem {
                    id: r.id,
                    name: r.name,
                    thing_type: r.thing_type,
                    description: r.description,
                    property_count,
                    action_count,
                    event_count,
                    is_builtin: r.is_builtin != 0,
                    category: r.category,
                    created_at: r.created_at,
                }
            })
            .collect())
    }

    /// Install (copy) a thing_template into the target workspace.
    ///
    /// Name conflict handling: if the template name already exists in the
    /// target workspace, appends " (来自市场)" suffix and retries.
    /// If the suffixed name also conflicts, appends a number.
    pub async fn install(db: &Db, template_id: &str, target_workspace_id: &str) -> Result<InstalledTemplate> {
        // 1. Fetch the source template
        let source = db
            .find_thing_template_full(template_id)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?
            .ok_or_else(|| MarketplaceError::NotFound(format!("thing_template {} not found", template_id)))?;

        // 2. Resolve name conflict in target workspace
        let ws_key = if target_workspace_id.is_empty() {
            ""
        } else {
            target_workspace_id
        };
        let final_name = resolve_template_name(db, ws_key, &source.name).await?;

        // 3. Insert copy with new id and workspace_id
        let new_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        db.insert_thing_template_copy(&source, &new_id, &final_name, target_workspace_id, &now)
            .await
            .map_err(|e| MarketplaceError::Template(e.to_string()))?;

        tracing::info!(
            "Installed thing_template {} as {} (id={}) in workspace {}",
            template_id,
            final_name,
            new_id,
            ws_key
        );

        Ok(InstalledTemplate {
            id: new_id,
            name: final_name,
        })
    }
}

// ──────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────

/// Count elements in a JSON array string. Returns 0 for malformed JSON.
fn json_array_len(s: &str) -> usize {
    serde_json::from_str::<Vec<serde_json::Value>>(s)
        .map(|v| v.len())
        .unwrap_or(0)
}

/// Resolve name conflicts in the target workspace.
///
/// Algorithm:
/// 1. If name does not exist → return name as-is.
/// 2. Append " (来自市场)" suffix → retry. If still no conflict → return that.
/// 3. Fallback: append " (N)" for N=1..99 until a free name is found.
async fn resolve_template_name(db: &Db, workspace_key: &str, name: &str) -> Result<String> {
    // Check original name
    if name_is_available(db, workspace_key, name).await {
        return Ok(name.to_string());
    }

    // Try " (来自市场)" suffix
    let suffixed = format!("{} (来自市场)", name);
    if name_is_available(db, workspace_key, &suffixed).await {
        return Ok(suffixed);
    }

    // Fallback: numbered suffix
    for i in 1..100 {
        let numbered = format!("{} ({})", name, i);
        if name_is_available(db, workspace_key, &numbered).await {
            return Ok(numbered);
        }
    }

    Err(MarketplaceError::Template(format!(
        "Unable to resolve name conflict for '{}' in workspace '{}'",
        name, workspace_key
    )))
}

/// Returns true if no row in thing_templates matches the workspace + name pair.
async fn name_is_available(db: &Db, workspace_key: &str, name: &str) -> bool {
    let result = db.count_thing_template_name_conflicts(workspace_key, name).await;
    match result {
        Ok(count) => count == 0,
        // If query fails (unlikely), err on the side of safety and report as unavailable
        Err(_) => false,
    }
}
