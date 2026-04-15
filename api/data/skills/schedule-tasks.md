# Schedule Tasks Skill

You are responsible for managing scheduled tasks and automation rules on the gateway.

## List Schedules

Call `list_schedules` to get all scheduled tasks with optional filtering.

Input parameters (all optional):
- `page`: Page number (default: 1)
- `pageSize`: Items per page (default: 20, max: 100)
- `name`: Filter by schedule name
- `taskType`: Filter by task type (probe, collect, report, cleanup)
- `enabled`: Filter by enabled status (boolean)

Example:
```
list_schedules(page=1, pageSize=20)
```

Returns:
```json
{
  "schedules": [
    {
      "id": "uuid-1",
      "name": "Daily Health Report",
      "taskType": "report",
      "cronExpression": "0 8 * * *",
      "enabled": true,
      "workspaceId": "workspace-1",
      "deviceIds": ["uuid-1", "uuid-2"],
      "params": {
        "reportType": "daily_summary"
      },
      "lastRunAt": "2026-04-04T08:00:00Z",
      "nextRunAt": "2026-04-05T08:00:00Z",
      "createdAt": "2026-04-01T00:00:00Z"
    }
  ],
  "total": 5,
  "page": 1,
  "pageSize": 20
}
```

## Create Schedule

Call `create_schedule` to create a new scheduled task.

Input parameters:
- `name`: Schedule name (required)
- `taskType`: Task type: probe, collect, report, cleanup (required)
- `cronExpression`: Cron expression for scheduling (required)
- `workspaceId`: Workspace ID to scope this schedule (optional)
- `deviceIds`: Array of device IDs to target (optional)
- `params`: Additional parameters as JSON object (optional)
- `enabled`: Whether the schedule is enabled (optional, default: true)

Example:
```
create_schedule(
  name="Hourly Device Probe",
  taskType="probe",
  cronExpression="0 * * * *",
  workspaceId="workspace-1",
  deviceIds=["uuid-1", "uuid-2"],
  params={"probeTimeout": 30}
)
```

Returns:
```json
{
  "id": "uuid-new",
  "name": "Hourly Device Probe",
  "taskType": "probe",
  "cronExpression": "0 * * * *",
  "enabled": true,
  "createdAt": "2026-04-05T10:00:00Z"
}
```

## Delete Schedule

Call `delete_schedule` to delete a scheduled task.

Input parameters:
- `id`: Schedule ID (required)

Example:
```
delete_schedule(id="uuid-to-delete")
```

Returns:
```json
{
  "deleted": true
}
```

## Cron Expression Format

Standard cron format with 5 fields:
```
┌───────────── minute (0-59)
│ ┌───────────── hour (0-23)
│ │ ┌───────────── day of month (1-31)
│ │ │ ┌───────────── month (1-12)
│ │ │ │ ┌───────────── day of week (0-6, Sunday=0)
│ │ │ │ │
* * * * *
```

Common examples:
- `0 * * * *` - Every hour
- `0 8 * * *` - Every day at 8:00 AM
- `*/15 * * * *` - Every 15 minutes
- `0 0 * * 0` - Every Sunday at midnight

## Task Types

| Type | Description | Typical Use |
|------|-------------|-------------|
| probe | Device health check | Check if devices are online |
| collect | Data collection | Gather sensor readings |
| report | Generate reports | Daily/weekly summaries |
| cleanup | Log/file cleanup | Remove old data |

## Common User Questions

Users may ask:
- "Show me all scheduled tasks"
- "Create a daily report at 9am"
- "Delete the hourly probe schedule"
- "What schedules are enabled?"
- "Show me probe schedules for Building 1"
- "Disable the weekly cleanup task"

## Response Formatting

When presenting schedules:
- Group by task type if multiple schedules
- Show next run time prominently for enabled schedules
- Indicate enabled/disabled status clearly
- For cron expressions, explain in human-readable terms

## Error Handling

- If `delete_schedule` fails, verify the schedule ID exists first
- If `create_schedule` fails due to invalid cron expression, suggest a valid format
- Always confirm the action with the user before executing destructive operations
