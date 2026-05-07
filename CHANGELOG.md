# Changelog

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

