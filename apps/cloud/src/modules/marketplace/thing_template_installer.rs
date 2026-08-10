// Thing Template Installer — local marketplace category for thing_templates.
// Lists and installs (copies) thing_templates from the local database,
// supporting workspace-scoped name conflict resolution.

use sqlx::{FromRow, SqlitePool};

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
// DB row (subset of thing_templates columns)
// ──────────────────────────────────────────────

#[derive(Debug, FromRow)]
struct ThingTemplateListRow {
    id: String,
    name: String,
    thing_type: String,
    description: Option<String>,
    properties: String,
    actions: String,
    events: String,
    is_builtin: i32,
    category: String,
    created_at: String,
}

#[derive(Debug, FromRow)]
struct ThingTemplateFullRow {
    name: String,
    display_name: String,
    description: Option<String>,
    version: String,
    author: Option<String>,
    category: String,
    manufacturer: Option<String>,
    device_type: String,
    thing_type: String,
    protocol_type: Option<String>,
    driver_name: Option<String>,
    tags: String,
    device_info: String,
    properties: String,
    actions: String,
    events: String,
    default_knowledge: Option<String>,
}

// ──────────────────────────────────────────────
// Installer
// ──────────────────────────────────────────────

pub struct ThingTemplateInstaller;

impl ThingTemplateInstaller {
    /// List thing_templates available as marketplace items.
    /// Shows built-in templates (workspace_id IS NULL) first,
    /// then workspace-scoped templates.
    pub async fn list(pool: &SqlitePool, workspace_id: &str) -> Result<Vec<ThingTemplateItem>> {
        let rows = sqlx::query_as::<_, ThingTemplateListRow>(
            "SELECT id, name, thing_type, description, properties, actions, events, \
             is_builtin, category, created_at \
             FROM thing_templates WHERE is_active = 1 \
             AND (workspace_id IS NULL OR workspace_id = ?) \
             ORDER BY is_builtin DESC, name",
        )
        .bind(workspace_id)
        .fetch_all(pool)
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
    pub async fn install(pool: &SqlitePool, template_id: &str, target_workspace_id: &str) -> Result<InstalledTemplate> {
        // 1. Fetch the source template
        let source = sqlx::query_as::<_, ThingTemplateFullRow>(
            "SELECT name, display_name, description, version, author, category, \
             manufacturer, device_type, thing_type, protocol_type, driver_name, \
             tags, device_info, properties, actions, events, default_knowledge \
             FROM thing_templates WHERE id = ? AND is_active = 1",
        )
        .bind(template_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| MarketplaceError::Template(e.to_string()))?
        .ok_or_else(|| MarketplaceError::NotFound(format!("thing_template {} not found", template_id)))?;

        // 2. Resolve name conflict in target workspace
        let ws_key = if target_workspace_id.is_empty() {
            ""
        } else {
            target_workspace_id
        };
        let final_name = resolve_template_name(pool, ws_key, &source.name).await?;

        // 3. Insert copy with new id and workspace_id
        let new_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();

        sqlx::query(
            "INSERT INTO thing_templates \
             (id, name, display_name, description, version, author, category, \
              manufacturer, device_type, thing_type, protocol_type, driver_name, \
              tags, device_info, properties, actions, events, default_knowledge, \
              is_builtin, is_active, workspace_id, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 1, ?, ?, ?)",
        )
        .bind(&new_id)
        .bind(&final_name)
        .bind(&source.display_name)
        .bind(&source.description)
        .bind(&source.version)
        .bind(&source.author)
        .bind(&source.category)
        .bind(&source.manufacturer)
        .bind(&source.device_type)
        .bind(&source.thing_type)
        .bind(&source.protocol_type)
        .bind(&source.driver_name)
        .bind(&source.tags)
        .bind(&source.device_info)
        .bind(&source.properties)
        .bind(&source.actions)
        .bind(&source.events)
        .bind(&source.default_knowledge)
        .bind(target_workspace_id)
        .bind(&now)
        .bind(&now)
        .execute(pool)
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
async fn resolve_template_name(pool: &SqlitePool, workspace_key: &str, name: &str) -> Result<String> {
    // Check original name
    if name_is_available(pool, workspace_key, name).await {
        return Ok(name.to_string());
    }

    // Try " (来自市场)" suffix
    let suffixed = format!("{} (来自市场)", name);
    if name_is_available(pool, workspace_key, &suffixed).await {
        return Ok(suffixed);
    }

    // Fallback: numbered suffix
    for i in 1..100 {
        let numbered = format!("{} ({})", name, i);
        if name_is_available(pool, workspace_key, &numbered).await {
            return Ok(numbered);
        }
    }

    Err(MarketplaceError::Template(format!(
        "Unable to resolve name conflict for '{}' in workspace '{}'",
        name, workspace_key
    )))
}

/// Returns true if no row in thing_templates matches the workspace + name pair.
async fn name_is_available(pool: &SqlitePool, workspace_key: &str, name: &str) -> bool {
    let result: std::result::Result<i64, sqlx::Error> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM thing_templates \
         WHERE COALESCE(workspace_id, '') = ? AND name = ?",
    )
    .bind(workspace_key)
    .bind(name)
    .fetch_one(pool)
    .await;
    match result {
        Ok(count) => count == 0,
        // If query fails (unlikely), err on the side of safety and report as unavailable
        Err(_) => false,
    }
}
