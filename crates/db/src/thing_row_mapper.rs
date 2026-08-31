use sqlx::Row;
use tinyiothub_core::error::Result;
use tinyiothub_core::models::thing::Thing;

/// Centralized SELECT column list for the `things` table.
pub const SELECT_COLUMNS: &str = r#"
    id, name, display_name, category, address, description, position,
    driver_name, device_model, protocol_type, factory_name, linked_data,
    driver_options, state, parent_id, template_id, workspace_id,
    linked_gateway, fingerprint, created_at, updated_at
"#;

/// Map a `SqliteRow` to a `Thing`.
pub fn row_to_thing(row: sqlx::sqlite::SqliteRow) -> Result<Thing> {
    let state_i32: i32 = row.get("state");
    Ok(Thing {
        id: row.get("id"),
        name: row.get("name"),
        display_name: row.get("display_name"),
        category: row.get("category"),
        address: row.get("address"),
        description: row.get("description"),
        position: row.get("position"),
        driver_name: row.get("driver_name"),
        device_model: row.get("device_model"),
        protocol_type: row.get("protocol_type"),
        factory_name: row.get("factory_name"),
        linked_gateway: row.get("linked_gateway"),
        fingerprint: row.get("fingerprint"),
        linked_data: row.get("linked_data"),
        driver_options: row.get("driver_options"),
        status: state_i32.into(),
        parent_id: row.get("parent_id"),
        template_id: row.get("template_id"),
        workspace_id: row.get("workspace_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        tags: None,
        properties: None,
        commands: None,
        last_heartbeat: None,
    })
}
