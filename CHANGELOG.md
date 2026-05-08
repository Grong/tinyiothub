# Changelog

## [0.2.0] - 2026-05-08

### Added
- C FFI driver hot-loading with `libloading`, `DynamicDeviceDriver`, and `DriverRegistry`
- Per-workspace driver isolation in `DriverRegistry` with `WorkspaceRegistry`
- Driver rehydration on startup from `driver_installations` database records
- `TemplateExporter` — export existing device as reusable `DeviceTemplate`
- `MarketplacePublisher` — publish device templates to marketplace.tinyiothub.com
- `/api/v1/devices/{id}/export-template` endpoint
- `/api/v1/devices/{id}/clone` endpoint
- `/api/v1/marketplace/publish/template` endpoint
- Driver health dashboard module with `/api/v1/driver-health/drivers` endpoint
- Workspace-scoped driver preference support via `workspace_driver_preferences` table
- Driver installation tracking with `driver_installations` table
- Integration test for `DriverRegistry` workspace isolation

### Fixed
- Export-template description handling (plain string vs JSON object)
- Removed raw SQL UPDATE from handler by adding `workspace_id` to `CreateDeviceTemplateRequest`
- Localized marketplace handler error messages to Chinese
- Registry write lock now released between rehydration iterations
- Removed redundant `driver_registry` field from `AppState` (uses global singleton)

## [0.1.3] - 2026-05-07

### Fixed
- Consistent workspace resolution across monitoring and auth handlers (#30)
- Role repository column name mismatch: `IsAdministrator` -> `is_administrator` (#37)
- Security config persistence deduplicated through `SecureEventService` layer (#40)
- Silent failure in `update_security_config` handler — now routes through service (#39)
- `sysinfo::System` cached in `AppState` to avoid per-request allocation (#42)
- Health check uses `count_devices()` instead of loading full device list (#38)
- Removed dead product handler tests from test suite (#37)

### Added
- Role permission handler tests with real permission IDs from migrations (#44)
- Monitoring handler tests for health endpoints with workspace seeding (#43)
- Admin gate tests for system metrics endpoint (#44)
- `SecureEventService.update_config()` and `save_config_to_db()` for atomic config updates

