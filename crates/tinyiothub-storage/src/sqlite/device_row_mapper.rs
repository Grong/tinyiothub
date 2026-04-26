use tinyiothub_core::models::device::Device;
use tinyiothub_core::error::Result;
use sqlx::Row;

/// Centralized SELECT column list for the `devices` table.
pub const SELECT_COLUMNS: &str = r#"
    id, name, display_name, device_type, address, description, position,
    driver_name, device_model, protocol_type, factory_name, linked_data,
    driver_options, state, parent_id, product_id, created_at, updated_at
"#;

/// Map a `SqliteRow` to a `Device`.
pub fn row_to_device(row: sqlx::sqlite::SqliteRow) -> Result<Device> {
    let state_i32: i32 = row.get("state");
    Ok(Device {
        id: row.get("id"),
        name: row.get("name"),
        display_name: row.get("display_name"),
        device_type: row.get("device_type"),
        address: row.get("address"),
        description: row.get("description"),
        position: row.get("position"),
        driver_name: row.get("driver_name"),
        device_model: row.get("device_model"),
        protocol_type: row.get("protocol_type"),
        factory_name: row.get("factory_name"),
        linked_data: row.get("linked_data"),
        driver_options: row.get("driver_options"),
        status: state_i32.into(),
        parent_id: row.get("parent_id"),
        product_id: row.get("product_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        tags: None,
        properties: None,
        commands: None,
        last_heartbeat: None,
    })
}
