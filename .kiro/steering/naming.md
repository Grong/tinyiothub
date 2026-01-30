---
inclusion: always
---

# Naming Conventions

## Core Principles

**CRITICAL**: Follow these naming patterns consistently across the codebase:

- **Files/Modules** → Nouns (what it represents)
- **Functions/Methods** → Verb phrases (what it does)  
- **Types/Structs** → Nouns (concepts)
- **Traits** → Capability adjectives (what it can do)

## File and Module Naming

### Rules
- Use `snake_case` format
- Use nouns to represent module content
- Avoid verbs (e.g., avoid `processing.rs`)

### Current Project Structure

```rust
// ✅ Correct examples (existing in project)
src/
├── api/               // API endpoints
│   ├── devices/       // Device management APIs
│   ├── monitoring/    // Monitoring APIs
│   └── auth/          // Authentication APIs
├── domain/            // Business logic
│   ├── device/        // Device domain
│   ├── event/         // Event domain
│   └── template/      // Template domain
├── infrastructure/    // External concerns
│   ├── persistence/   // Database layer
│   ├── gateway/       // Protocol implementations
│   └── messaging/     // Message brokers
└── shared/            // Cross-cutting concerns
    ├── error.rs       // Error types
    └── security/      // Security utilities

// ⚠️ Legacy naming (historical reasons - avoid in new code)
src/api/
├── deviceAlarm.rs         // Should be device_alarm.rs
└── deviceEventTrigger.rs  // Should be device_event_trigger.rs
```

### Architecture Layers

```rust
// Domain layer (business concepts)
domain/
├── device/            // Device management domain
├── event_service/     // Event processing domain
└── template/          // Template management domain

// Infrastructure layer (technical implementations)
infrastructure/
├── gateway/           // Protocol implementations (MQTT, Modbus, ONVIF)
├── persistence/       // Database operations
└── messaging/         // Message routing

// API layer (HTTP endpoints)
api/
├── devices/           // Device management endpoints
├── monitoring/        // System monitoring endpoints
└── auth/              // Authentication endpoints
```

## Function and Method Naming

### Query Methods (No Side Effects)

```rust
// Simple queries - use get/find/list prefixes
fn get_device_by_id(id: i64) -> Option<Device>
fn find_active_devices() -> Vec<Device>
fn list_all_users() -> Vec<User>

// Boolean queries - use is/has/can/should prefixes
fn is_online(&self) -> bool
fn has_permission(&self, perm: &str) -> bool
fn can_execute(&self) -> bool
fn should_retry(&self) -> bool

// Calculations/conversions
fn calculate_uptime(&self) -> Duration
fn to_json(&self) -> serde_json::Value
fn as_bytes(&self) -> &[u8]
```

### Command Methods (With Side Effects)

```rust
// Creation/initialization
fn new(config: Config) -> Self
fn create_device(data: DeviceData) -> Result<Device>
fn init_mqtt_client() -> Result<MqttClient>

// Modification/updates
fn set_status(&mut self, status: DeviceStatus)
fn update_config(&mut self, config: Config) -> Result<()>
fn add_tag(&mut self, tag: Tag)
fn remove_expired_data(&mut self)

// Async operations (common in project)
async fn fetch_device_data(id: i64) -> Result<DeviceData>
async fn send_mqtt_message(topic: &str, payload: &[u8]) -> Result<()>
async fn connect_to_device(addr: &str) -> Result<Connection>
```

### Precise Verb Selection

```rust
// ✅ Use specific verbs
fn persist_device(device: &Device) -> Result<()>     // Emphasizes storage
fn serialize_payload(data: &Data) -> Vec<u8>         // Emphasizes serialization
fn encode_modbus_frame(frame: &Frame) -> Bytes       // Emphasizes encoding
fn publish_mqtt_message(msg: &Message) -> Result<()> // Emphasizes publishing

// ❌ Avoid vague verbs
fn handle_device(device: &Device) -> Result<()>      // Too vague
fn process_data(data: &Data) -> Result<()>           // Avoid when possible
fn do_something(x: i32) -> Result<()>                // Never acceptable
```

## Type and Struct Naming

### Entity Types (PascalCase)

```rust
// Domain entities
struct Device { /* ... */ }
struct User { /* ... */ }
struct Product { /* ... */ }
struct Organization { /* ... */ }

// Configuration types
struct AppConfig { /* ... */ }
struct MqttConfig { /* ... */ }
struct DatabaseConfig { /* ... */ }

// Wrappers/containers
struct Cached<T> { /* ... */ }
struct Arc<T> { /* ... */ }
```

### API Struct Patterns

```rust
// API handlers (using Axum)
struct DeviceApi;
struct DeviceApiV2;      // Versioned APIs
struct UserApi;
struct MonitoringApi;

// Request/response models
struct LoginRequest { /* ... */ }
struct DeviceResponse { /* ... */ }
struct ApiResponse<T> { /* ... */ }
struct PaginationQuery { /* ... */ }
```

### Services and Managers

```rust
// Service classes (-Service suffix optional)
struct DataServer { /* ... */ }
struct MessageServer { /* ... */ }
struct EventService { /* ... */ }

// Clients (-Client suffix)
struct MqttClient { /* ... */ }
struct ModbusClient { /* ... */ }
struct OnvifClient { /* ... */ }

// Context/state
struct DataContext { /* ... */ }
struct AppState { /* ... */ }
```

## Trait Naming

```rust
// Capability traits (-able suffix)
trait Serializable { /* ... */ }
trait Configurable { /* ... */ }

// Role traits (-er/-or suffix)
trait DeviceDriver { /* ... */ }
trait EventHandler { /* ... */ }
trait DataCollector { /* ... */ }

// Standard library traits (follow Rust conventions)
trait Clone { /* ... */ }
trait Debug { /* ... */ }
trait Send { /* ... */ }
```

## Enum Naming

```rust
// State enums
enum DeviceStatus {
    Online,
    Offline,
    Connecting,
    Error { message: String },
}

// Event types
enum DeviceEvent {
    Connected,
    Disconnected,
    DataReceived { data: Vec<u8> },
    ErrorOccurred { error: String },
}

// Configuration options
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}
```

## Conversion Method Naming

```rust
// Consuming self conversions (into_)
fn into_string(self) -> String
fn into_bytes(self) -> Vec<u8>
fn into_inner(self) -> T

// Reference conversions
fn as_str(&self) -> &str           // Zero-cost view
fn as_bytes(&self) -> &[u8]        // Zero-cost view
fn to_string(&self) -> String      // May allocate memory
fn to_vec(&self) -> Vec<T>         // May allocate memory

// Type conversions
impl From<DeviceDto> for Device { /* ... */ }
impl TryFrom<String> for DeviceId { /* ... */ }
```

## Constants and Static Variables

```rust
// Constants (SCREAMING_SNAKE_CASE)
const MAX_RETRY_COUNT: u32 = 3;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MQTT_TOPIC_PREFIX: &str = "gateway/";

// Static variables
static GLOBAL_CONFIG: OnceCell<Config> = OnceCell::new();
static DB_POOL: Lazy<Pool> = Lazy::new(|| create_pool());
```

## Test Naming

```rust
#[cfg(test)]
mod tests {
    // Format: test_[subject]_[scenario]_[expected_result]
    #[test]
    fn test_device_creation_with_valid_data_succeeds() { /* ... */ }
    
    #[test]
    fn test_mqtt_client_with_invalid_host_fails() { /* ... */ }
    
    #[test]
    fn test_data_server_handles_concurrent_requests() { /* ... */ }
    
    // Test helper functions
    fn create_test_device() -> Device { /* ... */ }
    fn mock_mqtt_client() -> MockMqttClient { /* ... */ }
}
```

## Project-Specific Naming Patterns

### MQTT Related Naming

```rust
// Topic constants
const MQTT_TOPIC_HEARTBEAT: &str = "gateway/{sn}/heartbeat";
const MQTT_TOPIC_DEVICE_REGISTER: &str = "gateway/{sn}/device_register";
const MQTT_TOPIC_COMMAND: &str = "gateway/{sn}/command";

// MQTT functions
async fn publish_heartbeat(client: &MqttClient) -> Result<()>
async fn subscribe_to_commands(client: &MqttClient) -> Result<()>
fn format_mqtt_topic(sn: &str, suffix: &str) -> String
```

### Device Driver Naming

```rust
// Driver modules
mod modbus_driver;
mod onvif_driver;
mod snmp_driver;

// Driver implementations
struct ModbusRtuDriver { /* ... */ }
struct OnvifCameraDriver { /* ... */ }
struct SnmpDeviceDriver { /* ... */ }

// Driver methods
async fn read_registers(addr: u16, count: u16) -> Result<Vec<u16>>
async fn write_coil(addr: u16, value: bool) -> Result<()>
async fn get_camera_snapshot() -> Result<Vec<u8>>
```

### Frontend Naming (TypeScript/React)

```typescript
// Files: kebab-case
device-list.tsx
user-management.tsx
monitoring-dashboard.tsx

// Components: PascalCase
const DeviceList: React.FC = () => { /* ... */ }
const UserManagement: React.FC = () => { /* ... */ }
const MonitoringDashboard: React.FC = () => { /* ... */ }

// Variables/functions: camelCase
const deviceData = await fetchDeviceData()
const handleUserClick = (userId: string) => { /* ... */ }

// Types/interfaces: PascalCase
interface DeviceData { /* ... */ }
type UserRole = 'admin' | 'user'
```

## Naming Checklist

Before submitting code, verify:

- [ ] Module names use nouns
- [ ] Function names clearly express actions
- [ ] Type names reflect their purpose
- [ ] Avoid vague names (`handle`, `process`, `manager`)
- [ ] Consistent with existing project style
- [ ] Correct case format (`snake_case` vs `PascalCase`)
- [ ] Async functions use `async fn`
- [ ] Boolean functions use `is_/has_/can_/should_` prefixes
- [ ] API endpoints follow RESTful conventions
- [ ] Frontend components follow React conventions

## Legacy Code Patterns

**When encountering legacy naming, maintain consistency within the same module but use correct naming for new code:**

```rust
// Legacy patterns (avoid in new code)
deviceAlarm.rs          → device_alarm.rs
deviceEventTrigger.rs   → device_event_trigger.rs

// For backward compatibility, use pub use in new files:
// In new file device_alarm.rs:
pub use crate::api::deviceAlarm::*;
```

## Quick Reference

| Context | Format | Example |
|---------|--------|---------|
| Rust files/modules | `snake_case` | `device_service.rs` |
| Rust structs/enums | `PascalCase` | `DeviceStatus` |
| Rust functions | `snake_case` | `get_device_by_id` |
| Rust constants | `SCREAMING_SNAKE_CASE` | `MAX_RETRY_COUNT` |
| TypeScript files | `kebab-case` | `device-list.tsx` |
| React components | `PascalCase` | `DeviceList` |
| TypeScript variables | `camelCase` | `deviceData` |
| API endpoints | RESTful | `/api/v1/devices` |
