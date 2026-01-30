---
inclusion: always
---

# Project Structure & Architecture Guide

## Root Directory Layout

```
tinyiothub/
├── src/                    # Rust backend source code
├── web/                    # Next.js frontend application
├── derive/                 # Custom derive macros (edge_derive)
├── migrations/             # Database migration files
├── templates/              # Device template definitions
├── scripts/                # Python utility scripts
├── logs/                   # Application log files
├── app_settings.toml       # Application configuration
├── Cargo.toml             # Main Rust project manifest
└── *.db                   # SQLite database files
```

## Backend Architecture (`src/`)

### Clean Architecture Layers

The backend follows Domain-Driven Design (DDD) with clean architecture:

#### 1. Domain Layer (`src/domain/`)
Core business logic and entities:
- `device/` - Device management domain (entities, services, repositories)
- `alarm/` - Alarm and alerting domain
- `event/` - Legacy event system
- `event_service/` - New event service system with real-time capabilities
- `template/` - Device template management
- `user/`, `organization/`, `product/` - User management domains

#### 2. Application Layer (`src/application/`)
Application services and orchestration:
- `context.rs` - DataContext for dependency injection
- `data_server.rs` - Main data processing server
- `message_server.rs` - Message routing and processing
- `scheduler.rs` - Task scheduling (TimeTask)
- `service_manager.rs` - Background service management

#### 3. API Layer (`src/api/`)
HTTP REST API endpoints using Axum:
- `devices/` - Device management APIs
- `monitoring/` - Device monitoring and metrics
- `alarms/` - Alarm management
- `auth/` - Authentication (login, session)
- `users/` - User management
- `system/` - System configuration
- `templates/` - Device template APIs
- `middleware/` - JWT auth and context injection

#### 4. Infrastructure Layer (`src/infrastructure/`)
External concerns and adapters:
- `config/` - Configuration management system
- `persistence/` - Database connections and repositories
- `gateway/` - Protocol implementations (MQTT, Modbus, ONVIF)
- `hardware/` - Hardware abstraction (GPIO, display)
- `messaging/` - Message broker integrations

#### 5. Shared Layer (`src/shared/`)
Cross-cutting concerns:
- `app_state.rs` - Application state management
- `error.rs` - Error types and handling
- `security/` - JWT and authentication utilities

#### 6. DTOs (`src/dto/`)
Data transfer objects for API serialization:
- `entity/` - Database entity DTOs
- `request/` - API request models
- `response/` - API response wrappers (ApiResponse<T>)

## Frontend Architecture (`web/`)

### Next.js Application Structure

```
web/
├── app/                    # Next.js App Router
│   ├── (commonLayout)/     # Shared layout pages
│   ├── components/         # React components
│   └── styles/            # Global styles
├── service/               # API service layer (required pattern)
├── hooks/                 # Custom React hooks
├── lib/                   # Utilities (api-client, query-keys)
├── context/               # React context providers
├── store/                 # Zustand state management
├── types/                 # TypeScript type definitions
├── i18n/                  # Internationalization files
└── utils/                 # Frontend utilities
```

### Key Frontend Patterns

1. **Service Layer Pattern**: All API calls must go through service files
2. **React Query Integration**: Use hooks for data fetching and caching
3. **Unified API Client**: Use `lib/api-client.ts` for all HTTP requests
4. **Component Organization**: Group by feature, not by type

## Development Patterns

### API Development
- All endpoints return `Json<ApiResponse<T>>` format
- Use `ApiResponseBuilder` for consistent responses
- JWT middleware for protected routes
- Axum extractors for request parsing

### Database Operations
- SQLx for compile-time checked queries
- Repository pattern in domain layer
- Migration-driven schema changes
- Connection pooling via DataContext

### Event System
- Legacy event system in `src/domain/event/`
- New event service in `src/domain/event_service/`
- Real-time notifications and status updates
- Event-driven architecture for device state changes

### Device Driver Architecture
- Protocol-agnostic driver interface
- Modbus RTU/TCP, ONVIF, SNMP support
- Template-based device configuration
- Async I/O with Tokio runtime

## Naming Conventions

### Backend (Rust)
- **Files/Modules**: `snake_case` (e.g., `device_service.rs`)
- **Structs/Enums**: `PascalCase` (e.g., `DeviceStatus`)
- **Functions**: `snake_case` (e.g., `get_device_by_id`)
- **Constants**: `SCREAMING_SNAKE_CASE`

### Frontend (TypeScript)
- **Files**: `kebab-case` (e.g., `device-list.tsx`)
- **Components**: `PascalCase` (e.g., `DeviceList`)
- **Variables/Functions**: `camelCase`
- **Types/Interfaces**: `PascalCase`

### API Conventions
- **Endpoints**: RESTful with `/api/v1/` prefix
- **Request Fields**: `snake_case` (backend)
- **Response Fields**: Auto-converted to `camelCase` (frontend)

## Adding New Features

### Backend API Endpoint
1. Create domain entities in `src/domain/[domain]/`
2. Implement repository in `src/infrastructure/persistence/repository/`
3. Create API handlers in `src/api/[domain]/`
4. Add DTOs in `src/dto/entity/` and `src/dto/response/`
5. Register routes in `src/api/mod.rs`

### Frontend Service Integration
1. Define TypeScript types in `web/types/`
2. Create service file in `web/service/[feature].ts`
3. Implement React Query hooks
4. Create components in `web/app/components/[feature]/`
5. Add query keys to `web/lib/query-keys.ts`

### Database Changes
1. Create migration in `migrations/YYYYMMDD_description.sql`
2. Update entity DTOs in `src/dto/entity/`
3. Implement repository methods
4. Run migration with `sqlx migrate run`

## Configuration Management

### Settings Hierarchy
1. `app_settings.toml` - Base configuration
2. Environment variables - Runtime overrides
3. `web/.env.local` - Frontend environment variables

### Key Configuration Areas
- Database connection settings
- MQTT broker configuration
- JWT secret and expiration
- Logging levels and file paths
- Hardware GPIO pin mappings

## Testing Strategy

### Backend Testing
- Unit tests for domain logic
- Integration tests for API endpoints
- Use `#[tokio::test]` for async tests
- Mock external dependencies

### Frontend Testing
- Jest for unit testing
- React Testing Library for components
- API mocking with MSW (if needed)

## Performance Considerations

### Backend Optimization
- Tokio async runtime (10 worker threads)
- Connection pooling for database
- Efficient protocol implementations
- Structured logging with tracing

### Frontend Optimization
- React Query for caching and deduplication
- Code splitting with Next.js
- Optimized bundle analysis
- Lazy loading for large components

## Security Patterns

### Authentication Flow
1. Login via `/api/v1/auth/login`
2. JWT token storage in HTTP-only cookies
3. Middleware validation on protected routes
4. Automatic token refresh handling

### Data Validation
- Zod schemas for frontend validation
- Serde validation for backend DTOs
- SQL injection prevention with SQLx
- XSS protection with DOMPurify
