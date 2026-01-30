---
inclusion: always
---

# Technology Stack & Development Guidelines

## Core Technology Stack

### Language & Runtime
- **Rust 2021 Edition** - Primary backend language with strict type safety
- **Tokio** - Async runtime (10 worker threads) for high-performance I/O
- **TypeScript/React** - Frontend with Next.js framework

### Backend Framework
- **Axum** - Modern async web framework with excellent performance
- **Tower** - Service abstraction layer for middleware composition
- **Tower-HTTP** - HTTP middleware (CORS, tracing, static file serving)

### Key Dependencies

#### Communication Protocols
- `tokio-modbus` - Modbus RTU/TCP protocol (local fork with custom features)
- `onvif` - ONVIF camera integration (local fork)
- `rumqttc` - MQTT client for IoT device communication
- `snmp` - SNMP protocol support for network devices
- `tokio-serial` / `serialport` - Serial port communication

#### Database & Persistence
- `rusqlite` - SQLite database (local fork with bundled-full feature)
- `sqlx` - Async SQL toolkit with compile-time query validation
- Database connection configured in `app_settings.toml`
- Migration files in `migrations/` directory

#### Authentication & Security
- `jsonwebtoken` - JWT token handling for API authentication
- `openssl` - Cryptographic operations (vendored for cross-compilation)

#### Serialization & Utilities
- `serde` / `serde_json` - Serialization framework
- `chrono` - Date/time handling with timezone support
- `tracing` / `tracing-subscriber` - Structured logging
- `reqwest` - HTTP client for external API calls
- `cron` - Task scheduling for periodic operations

## Development Patterns

### Async Programming
- All I/O operations must use async/await
- Use `tokio::spawn` for concurrent tasks
- Prefer `Arc<Mutex<T>>` for shared state in async contexts
- Handle timeouts with `tokio::time::timeout`

### Error Handling
- Use `Result<T, E>` for fallible operations
- Create custom error types with `thiserror` derive
- Log errors with appropriate levels using `tracing`
- Return structured API errors using `ApiResponse<T>`

### Database Operations
- Use SQLx for compile-time checked queries
- Implement repository pattern for data access
- Use transactions for multi-step operations
- Handle database migrations through `migrations/` files

### API Development
- All endpoints return `Json<ApiResponse<T>>` format
- Use Axum extractors for request parsing
- Implement JWT middleware for protected routes
- Follow RESTful conventions for URL design

## Build & Development Commands

### Local Development
```bash
# Format and check code
cargo fmt && cargo clippy

# Run with detailed logging
RUST_LOG=debug cargo run

# Run tests
cargo test

# Check compilation without building
cargo check
```

### Database Operations
```bash
# Run migrations
sqlx migrate run

# Create new migration
sqlx migrate add <migration_name>
```

### Cross-Compilation
**Primary targets:**
- `armv7-unknown-linux-gnueabihf` - ARM Linux devices
- `armv7-linux-androideabi` - ARM Android systems

```bash
# Cross-compile for ARM
cross build --target armv7-unknown-linux-gnueabihf --release
```

## Code Quality Standards

### Allowed Lints
The project permits these lints for development flexibility:
- `dead_code`, `unused_imports`, `unused_variables`, `unused_mut`
- `non_snake_case` - Supports mixed naming conventions

### Testing Requirements
- Unit tests for business logic
- Integration tests for API endpoints
- Use `#[tokio::test]` for async tests
- Mock external dependencies in tests

### Documentation
- Document public APIs with rustdoc comments
- Include examples in documentation
- Maintain README files for complex modules

## Configuration Management

### Application Settings
- `app_settings.toml` - Main configuration file
- Environment-specific overrides supported
- MQTT broker settings, database paths, server ports
- Device driver configurations

### Environment Variables
- `RUST_LOG` - Controls logging level (default: "info")
- `DATABASE_URL` - Override database connection
- `JWT_SECRET` - JWT signing key for production

## Local Forks & Dependencies

### Custom Forks
The project maintains local forks with specific modifications:
- `tokio-modbus/` - Enhanced Modbus protocol support
- `onvif/` - ONVIF camera integration improvements
- `rusqlite/` - SQLite with bundled features
- `derive/` - Custom procedural macros

### Dependency Management
- Use `path` dependencies for local forks in `Cargo.toml`
- Keep forks synchronized with upstream when possible
- Document custom modifications in fork README files

## Performance Considerations

### Async Runtime
- Tokio configured with 10 worker threads
- Use `spawn_blocking` for CPU-intensive tasks
- Avoid blocking operations in async contexts

### Memory Management
- Use `Arc` for shared immutable data
- Prefer `Cow<str>` for string handling when appropriate
- Monitor memory usage in long-running processes

### Database Performance
- Use connection pooling for database access
- Implement proper indexing for query performance
- Use prepared statements through SQLx
