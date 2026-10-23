// Thing Handler Tests
// Tests for device MCP tool handlers

use serde_json::json;

/// Test get_thing handler returns error for non-existent thing
#[tokio::test]
async fn test_get_thing_not_found() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("get_thing").unwrap();

    let result = handler.execute(json!({"id": "nonexistent-device-id"})).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    assert!(matches!(
        err,
        crate::domains::mcp::ToolError::NotFound(_) | crate::domains::mcp::ToolError::Internal(_)
    ));
}

/// Test search_things handler returns valid response (or graceful error)
#[tokio::test]
async fn test_search_things_returns_response() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler.execute(json!({"keyword": "test"})).await;

    match result {
        Ok(value) => {
            assert!(value.is_object(), "search_things should return an object");
        }
        Err(e) => {
            assert!(
                matches!(e, crate::domains::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test search_things accepts keyword and limit parameters
#[tokio::test]
async fn test_search_things_with_params() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler
        .execute(json!({
            "keyword": "sensor",
            "limit": 10
        }))
        .await;
    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::domains::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }

    let result = handler
        .execute(json!({
            "keyword": "modbus",
            "tag": "production"
        }))
        .await;
    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::domains::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }

    // Empty keyword is now allowed — returns all things (same behavior as non-empty)
    let result = handler.execute(json!({"keyword": ""})).await;
    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::domains::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test search_things with tag filter
#[tokio::test]
async fn test_search_things_with_tag() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("search_things").unwrap();

    let result = handler
        .execute(json!({
            "keyword": "temp",
            "tag": "critical"
        }))
        .await;

    match result {
        Ok(_) => {}
        Err(e) => {
            assert!(
                matches!(e, crate::domains::mcp::ToolError::Internal(_)),
                "Expected Internal error for uninitialized state, got {:?}",
                e
            );
        }
    }
}

/// Test read_properties handler metadata
#[tokio::test]
async fn test_read_properties_handler_metadata() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("read_properties").unwrap();

    assert_eq!(handler.name(), "read_properties");
    assert!(!handler.description().is_empty());
}

/// Test write_properties handler metadata
#[tokio::test]
async fn test_write_properties_handler_metadata() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("write_properties").unwrap();

    assert_eq!(handler.name(), "write_properties");
    assert!(!handler.description().is_empty());
}

/// Test send_command handler metadata
#[tokio::test]
async fn test_send_command_handler_metadata() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("send_command").unwrap();

    assert_eq!(handler.name(), "send_command");
    assert!(!handler.description().is_empty());
}

/// Test create_thing handler metadata
#[tokio::test]
async fn test_create_thing_handler_metadata() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("create_thing").unwrap();

    assert_eq!(handler.name(), "create_thing");
    assert!(!handler.description().is_empty());

    let schema = handler.input_schema();
    let json_schema = schema.to_json();
    let required = json_schema["required"].as_array().unwrap();
    assert!(
        required.iter().any(|r| r == "name"),
        "name should be required for create_thing"
    );
}

/// Test delete_thing handler metadata
#[tokio::test]
async fn test_delete_thing_handler_metadata() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("delete_thing").unwrap();

    assert_eq!(handler.name(), "delete_thing");
    assert!(!handler.description().is_empty());
}

// ── C1 regression pins: advertised `thingId` primary key + legacy `deviceId` alias ──
// Without initialized AppState the tools fail *after* parsing, so any error
// other than InvalidParams proves the advertised key was accepted.

#[tokio::test]
async fn test_read_properties_accepts_advertised_thing_id_key() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("read_properties").unwrap();

    let result = handler
        .execute(json!({"thingId": "dev-1", "propertyName": "temperature"}))
        .await;
    if let Err(crate::domains::mcp::ToolError::InvalidParams(e)) = result {
        panic!("advertised thingId key must deserialize, got InvalidParams: {}", e);
    }

    // Legacy alias: deviceId must still deserialize.
    let result = handler
        .execute(json!({"deviceId": "dev-1", "propertyName": "temperature"}))
        .await;
    if let Err(crate::domains::mcp::ToolError::InvalidParams(e)) = result {
        panic!("legacy deviceId alias must deserialize, got InvalidParams: {}", e);
    }
}

/// E4b §3.2：MCP read_properties 成功读取写入共享 DemandTracker；失败不写。
#[tokio::test]
async fn read_properties_records_successful_reads_as_demand() {
    use crate::domains::mcp::handlers::{McpAuthContext, with_mcp_context};
    use crate::domains::mcp::tool_registry::ToolHandler;
    use crate::shared::access_ports::{
        DbAcceptanceStore, DbDeviceStore, DbDriverStore, DbProjectionStore, DbThingSource,
    };
    use axum::extract::FromRef;
    use std::sync::Arc;
    use tinyiothub_runtime::access::demand::DemandTracker;
    use tinyiothub_runtime::access::registry::ManifestRegistry;
    use tinyiothub_runtime::access::service::AccessService;

    let (mut app_state, pool) = crate::test_utils::setup_test_app_with_pool().await;
    crate::test_utils::seed_test_workspace(&pool, "tenant-1", "ws").await;
    sqlx::query("INSERT INTO things (id, name, workspace_id, thing_type) VALUES ('t1','T1','ws','device')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO thing_properties (id, thing_id, name) VALUES ('p1','t1','温度')")
        .execute(&pool)
        .await
        .unwrap();

    let demand = Arc::new(DemandTracker::new());
    let db = app_state.db.clone();
    let data = tempfile::tempdir().unwrap();
    app_state.set_access(Arc::new(AccessService::new(
        data.path().to_path_buf(),
        Arc::new(ManifestRegistry::new()),
        Arc::new(DbDriverStore(db.clone())),
        Arc::new(DbDeviceStore(db.clone())),
        Arc::new(DbProjectionStore(db.clone())),
        Arc::new(DbThingSource(db.clone())),
        Arc::new(DbAcceptanceStore(db)),
        demand.clone(),
    )));

    let mcp_state = Arc::new(<crate::domains::mcp::McpState as FromRef<crate::state::AppState>>::from_ref(&app_state));
    let handler = crate::domains::mcp::tools::thing::ThingPropertyGetHandler::new(Some(mcp_state));

    let result = with_mcp_context(McpAuthContext::for_jwt("ws".into(), "u1".into()), || {
        handler.execute(json!({"thingId": "t1", "propertyName": "温度"}))
    })
    .await;
    assert!(result.is_ok(), "{:?}", result.err());
    assert!(demand.has_demand_for_property("ws", "t1", "温度"));

    // Unknown property → NotFound and no demand (clear the successful read first).
    demand.gc_at(std::time::Instant::now() + std::time::Duration::from_secs(120));
    assert!(!demand.has_demand_for_property("ws", "t1", "温度"));
    let result = with_mcp_context(McpAuthContext::for_jwt("ws".into(), "u1".into()), || {
        handler.execute(json!({"thingId": "t1", "propertyName": "不存在"}))
    })
    .await;
    assert!(matches!(result, Err(crate::domains::mcp::ToolError::NotFound(_))));
    assert!(!demand.has_demand_for_property("ws", "t1", "温度"));
}

#[tokio::test]
async fn test_write_properties_accepts_advertised_thing_id_key() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("write_properties").unwrap();

    let result = handler
        .execute(json!({"thingId": "dev-1", "properties": {"target_temp": "22"}}))
        .await;
    if let Err(crate::domains::mcp::ToolError::InvalidParams(e)) = result {
        panic!("advertised thingId key must deserialize, got InvalidParams: {}", e);
    }

    // Legacy alias: deviceId must still deserialize.
    let result = handler
        .execute(json!({"deviceId": "dev-1", "properties": {"target_temp": "22"}}))
        .await;
    if let Err(crate::domains::mcp::ToolError::InvalidParams(e)) = result {
        panic!("legacy deviceId alias must deserialize, got InvalidParams: {}", e);
    }
}

#[tokio::test]
async fn test_send_command_accepts_advertised_thing_id_key() {
    crate::domains::mcp::register_tools(None).await;
    let registry = crate::domains::mcp::get_mcp_registry().unwrap();
    let guard = registry.read().await;
    let handler = guard.get("send_command").unwrap();

    let result = handler
        .execute(json!({"thingId": "dev-1", "commandName": "reboot"}))
        .await;
    if let Err(crate::domains::mcp::ToolError::InvalidParams(e)) = result {
        panic!("advertised thingId key must deserialize, got InvalidParams: {}", e);
    }

    // Legacy alias: deviceId must still deserialize.
    let result = handler
        .execute(json!({"deviceId": "dev-1", "commandName": "reboot"}))
        .await;
    if let Err(crate::domains::mcp::ToolError::InvalidParams(e)) = result {
        panic!("legacy deviceId alias must deserialize, got InvalidParams: {}", e);
    }
}
