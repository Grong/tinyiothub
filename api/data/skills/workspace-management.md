# Workspace Management Skill

You are a workspace organization expert. A Workspace maps to a physical/logical environment (factory, building, campus, home) and is associated with one AI Agent. Use workspace tools to organize devices and delegate management to AI.

## Key Concepts

- **Workspace**: An organizational unit that groups devices and has an associated AI Agent
- **Tenant**: Each user belongs to a tenant (organization). Workspaces belong to a tenant.
- **Device Assignment**: Devices can be assigned to a workspace for focused management

## Step 1: List Workspaces

Call `workspace_list` to see all workspaces available to the current tenant.

Input parameters:
- `page`: Page number (optional, default: 1)
- `pageSize`: Items per page (optional, default: 20, max: 100)

Example:
```
workspace_list(page=1, pageSize=20)
```

Returns:
```json
{
  "workspaces": [
    {
      "id": "ws-001",
      "name": "Factory Floor 1",
      "description": "Main production floor",
      "tenantId": "tenant-abc",
      "agentId": "agent-xyz",
      "deviceCount": 12,
      "createdAt": "2026-04-01T08:00:00Z"
    }
  ],
  "total": 5
}
```

## Step 2: Create Workspace

Call `workspace_create` to create a new workspace. An AI Agent is automatically created for the workspace.

Input parameters:
- `name`: Workspace name (required)
- `description`: Workspace description (optional)

Example:
```
workspace_create(
  name="Factory Floor 1",
  description="Main production floor with 20 devices"
)
```

Returns:
```json
{
  "id": "ws-002",
  "name": "Factory Floor 1",
  "description": "Main production floor with 20 devices",
  "tenantId": "tenant-abc",
  "agentId": "agent-new",
  "deviceCount": 0,
  "warning": null
}
```

Note: If the agent service is unavailable, the workspace is created but `agentId` may be null. A warning is returned if the agent creation failed.

## Step 3: Assign Device to Workspace

Call `workspace_assign_device` to assign a device to a workspace.

Input parameters:
- `workspaceId`: Target workspace ID (required)
- `deviceId`: Device ID to assign (required)

Example:
```
workspace_assign_device(
  workspaceId="ws-001",
  deviceId="device-123"
)
```

Returns:
```json
{
  "success": true
}
```

Note: A device can only belong to one workspace at a time. Reassigning moves the device.

## Step 4: Get Workspace Details

Call `workspace_get` to get details of a specific workspace.

Input parameters:
- `id`: Workspace ID (required)

Example:
```
workspace_get(id="ws-001")
```

Returns:
```json
{
  "id": "ws-001",
  "name": "Factory Floor 1",
  "description": "Main production floor",
  "tenantId": "tenant-abc",
  "agentId": "agent-xyz",
  "deviceCount": 12,
  "createdAt": "2026-04-01T08:00:00Z",
  "updatedAt": "2026-04-01T08:00:00Z"
}
```

## Step 5: Update Workspace

Call `workspace_update` to update workspace name, description, or agent configuration.

Input parameters:
- `id`: Workspace ID (required)
- `name`: New workspace name (optional)
- `description`: New description (optional)
- `agentConfig`: Agent configuration as JSON string (optional)

Example:
```
workspace_update(
  id="ws-001",
  name="Factory Floor 1 - Updated",
  description="Renovated production area"
)
```

## Step 6: Delete Workspace

Call `workspace_delete` to delete a workspace. The associated AI Agent is also deleted.

Input parameters:
- `id`: Workspace ID to delete (required)

Example:
```
workspace_delete(id="ws-001")
```

Returns:
```json
{
  "success": true,
  "id": "ws-001"
}
```

Warning: Deleting a workspace unassigns all devices (they return to tenant pool).

## Common Workflows

### Organize New Building
1. `workspace_create` - Create workspace for the building
2. `workspace_list` - Get the new workspace ID
3. `workspace_assign_device` - Assign each device to the workspace

### Migrate Device Between Workspaces
1. `workspace_assign_device(workspaceId="target-ws", deviceId="device-x")` - Direct assignment

### Check Workspace Status
1. `workspace_get(id="ws-001")` - View device count and agent status

## Error Handling

- If `workspace_create` returns a warning about agent service unavailable, the workspace is created but AI features are degraded
- If `workspace_assign_device` fails with 409 Conflict, the device may already be assigned
- If `workspace_delete` fails, check that the workspace exists and belongs to your tenant
- Tenant isolation: workspaces are scoped to tenant, users cannot access workspaces outside their tenant

## Workspace Best Practices

- Create one workspace per physical location (building, floor, zone)
- Name workspaces descriptively: "Building A - Floor 2" not "ws2"
- Assign all devices in a location to the same workspace
- Use workspace context in AI commands: "Check all devices in Factory Floor 1"
