use tinyiothub_runtime::driver::registry::DriverRegistry;

#[test]
fn test_workspace_isolation() {
    let reg = DriverRegistry::new();

    let ws_a = "ws-a";
    let ws_b = "ws-b";

    // No drivers initially.
    assert!(reg.find(ws_a, "modbus").is_none());
    assert!(reg.find(ws_b, "modbus").is_none());
    assert_eq!(reg.list_workspaces().len(), 0);

    // list_for_workspace returns empty when workspace has no drivers.
    assert!(reg.list_for_workspace(ws_a).is_empty());
}
