use tinyiothub_web::response::ApiResponseBuilder;
use crate::modules::user::CreateUserRequest;
use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{
    shared::api_response::ApiResponse,
    shared::{app_state::AppState, error::Result}
};

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InitializeRequest {
    pub admin_username: String,
    pub admin_password: String,
    pub admin_email: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub struct InitializeResponse {
    pub success: bool,
    pub message: String,
    pub admin_user_id: Option<String>,
}

pub fn create_router() -> Router<AppState> {
    Router::new().route("/initialize", post(initialize_system))
}

/// 初始化系统 - 创建默认管理员用户
async fn initialize_system(
    State(state): State<AppState>,
    Json(request): Json<InitializeRequest>,
) -> Json<ApiResponse<InitializeResponse>> {
    // 检查是否已经有用户存在
    match state.user_service.find_all(&Default::default()).await {
        Ok(users) if !users.is_empty() => {
            return ApiResponseBuilder::success(InitializeResponse {
                success: false,
                message: "系统已经初始化，存在用户账户".to_string(),
                admin_user_id: None,
            });
        }
        Ok(_) => {
            // 没有用户，可以初始化
        }
        Err(e) => {
            tracing::error!("Failed to check existing users: {}", e);
            return ApiResponseBuilder::error("系统初始化失败".to_string());
        }
    }

    // 验证输入
    if request.admin_username.trim().is_empty() {
        return ApiResponseBuilder::error("管理员用户名不能为空".to_string());
    }

    if request.admin_password.len() < 6 {
        return ApiResponseBuilder::error("管理员密码长度不能少于6位".to_string());
    }

    // 创建管理员用户
    let create_request = CreateUserRequest {
        username: request.admin_username.clone(),
        password: request.admin_password,
        phone: None,
        email: request.admin_email,
        display_name: None,
        is_enabled: Some(true), // 启用状态
        parent_id: None
};

    match state.user_service.create_user(&create_request).await {
        Ok(admin_user) => {
            tracing::info!("System initialized with admin user: {}", admin_user.get_display_name());

            ApiResponseBuilder::success(InitializeResponse {
                success: true,
                message: "系统初始化成功".to_string(),
                admin_user_id: Some(admin_user.id),
            })
        }
        Err(e) => {
            tracing::error!("Failed to create admin user: {}", e);
            ApiResponseBuilder::error("创建管理员用户失败".to_string())
        }
    }
}

/// 检查系统是否需要初始化
#[allow(dead_code)]
pub async fn check_system_initialization(state: &AppState) -> Result<bool> {
    let users = state.user_service.find_all(&Default::default()).await?;
    Ok(users.is_empty())
}

/// 自动创建默认管理员用户（如果不存在），并确保默认租户和工作空间
pub async fn ensure_default_admin_user(state: &AppState) -> Result<()> {
    // 先查找 admin 用户是否已存在
    let admin_user = state.user_service.get_user_by_username("admin").await?;

    let admin_user_id = if let Some(user) = admin_user {
        // admin 用户已存在，检查密码哈希是否是迁移脚本里的假哈希
        if user.password_hash == "FIX_ME_admin_hash" || user.password_hash == "hashed_admin123" {
            tracing::info!("[init] Admin user has invalid password hash from migration, fixing...");
            match state.user_service.update_password(&user.id, "admin123").await {
                Ok(_) => {
                    tracing::info!("[init] Admin password fixed successfully");
                }
                Err(e) => {
                    tracing::error!("[init] Failed to fix admin password: {}", e);
                    return Err(e);
                }
            }
        }
        user.id
    } else {
        // 创建默认管理员用户
        tracing::info!("[init] No admin user found, creating default admin...");
        let create_request = CreateUserRequest {
            username: "admin".to_string(),
            password: "admin123".to_string(),
            phone: None,
            email: Some("admin@tinyiothub.local".to_string()),
            display_name: Some("Administrator".to_string()),
            is_enabled: Some(true),
            parent_id: None
};

        match state.user_service.create_user(&create_request).await {
            Ok(admin_user) => {
                tracing::info!(
                    "Created default admin user: {} (ID: {})",
                    admin_user.get_display_name(),
                    admin_user.id
                );
                tracing::warn!(
                    "Default admin password is 'admin123', please change it immediately!"
                );
                admin_user.id
            }
            Err(e) => {
                tracing::error!("Failed to create default admin user: {}", e);
                return Err(e);
            }
        }
    };

    // 确保默认租户和工作空间/Agent 存在
    ensure_default_tenant(state, &admin_user_id).await?;

    Ok(())
}

/// 确保任意用户关联到默认租户和工作空间（幂等，可多次调用）
pub async fn ensure_user_has_workspace(state: &AppState, user_id: &str) -> Result<()> {
    ensure_default_tenant(state, user_id).await
}

/// 确保默认租户、租户用户关联和默认工作空间/Agent 存在
async fn ensure_default_tenant(state: &AppState, user_id: &str) -> Result<()> {
    let pool = state.database().pool();

    // 检查 admin 是否已有租户
    let has_tenant: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM tenant_users WHERE user_id = ?)"
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

    if !has_tenant {
        tracing::info!("[init] User has no tenant, bootstrapping default tenant...");

        // 检查是否有任何租户
        let tenant_exists: bool = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM tenants WHERE id = 'tenant-default-001')"
        )
        .fetch_one(pool)
        .await
        .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

        if !tenant_exists {
            // 创建默认租户
            sqlx::query(
                r#"INSERT INTO tenants
                   (id, name, slug, status, plan_id, subscription_status,
                    billing_email, timezone, locale, created_at, updated_at)
                   VALUES
                   ('tenant-default-001', 'Default Organization', 'default', 'active',
                    'plan_free', 'active', 'admin@tinyiothub.local', 'UTC', 'zh-CN',
                    datetime('now'), datetime('now'))"#
            )
            .execute(pool)
            .await
            .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;
            tracing::info!("[init] Created default tenant");
        }

        // 关联 admin 用户到默认租户
        let tenant_user_id = format!("tu-{}", user_id);
        sqlx::query(
            r#"INSERT OR IGNORE INTO tenant_users
               (id, tenant_id, user_id, role, invitation_status, joined_at, created_at, updated_at)
               VALUES (?, 'tenant-default-001', ?, 'owner', 'accepted',
                       datetime('now'), datetime('now'), datetime('now'))"#
        )
        .bind(&tenant_user_id)
        .bind(user_id)
        .execute(pool)
        .await
        .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;
        tracing::info!("[init] Linked admin user to default tenant");
    }

    // 确保默认工作空间和 Agent 存在（无论租户是否新建）
    ensure_default_workspace_and_agent(state, pool).await?;

    // 将未分配的设备归属到默认租户和工作空间
    sqlx::query(
        "UPDATE devices SET tenant_id = 'tenant-default-001' WHERE tenant_id IS NULL"
    )
    .execute(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

    sqlx::query(
        "UPDATE devices SET workspace_id = 'ws-default-001' WHERE workspace_id IS NULL AND tenant_id = 'tenant-default-001'"
    )
    .execute(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;
    tracing::info!("[init] Assigned orphan devices to default tenant/workspace");

    Ok(())
}

/// 确保默认工作空间存在，若无则创建并同步创建 Agent；若存在但缺少 agent_id 则 backfill
async fn ensure_default_workspace_and_agent(
    state: &AppState,
    pool: &sqlx::Pool<sqlx::Sqlite>,
) -> Result<()> {
    // 检查默认工作空间是否存在
    let ws_exists: bool = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM workspaces WHERE id = 'ws-default-001')"
    )
    .fetch_one(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

    if !ws_exists {
        sqlx::query(
            r#"INSERT INTO workspaces
               (id, name, description, tenant_id, created_at, updated_at)
               VALUES
               ('ws-default-001', '默认工作空间', '系统自动创建的默认工作空间',
                'tenant-default-001', datetime('now'), datetime('now'))"#
        )
        .execute(pool)
        .await
        .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;
        tracing::info!("[init] Created default workspace");
    }

    // 检查 workspace 是否缺少 agent_id
    let needs_agent: bool = sqlx::query_scalar::<_, bool>(
        "SELECT (agent_id IS NULL OR agent_id = '') FROM workspaces WHERE id = 'ws-default-001'"
    )
    .fetch_one(pool)
    .await
    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;

    if needs_agent {
        let agent_result = state
            .agent_runtime
            .create_agent(&crate::shared::agent::AgentConfig {
                workspace_id: "ws-default-001".to_string(),
                name: "默认工作空间".to_string(),
                ..Default::default()
            })
            .await;

        match agent_result {
            Ok(agent_id) => {
                sqlx::query("UPDATE workspaces SET agent_id = ? WHERE id = 'ws-default-001'")
                    .bind(&agent_id)
                    .execute(pool)
                    .await
                    .map_err(|e| crate::shared::error::Error::DatabaseError(e.to_string()))?;
                tracing::info!("[init] Created agent {} for default workspace", agent_id);
            }
            Err(e) => {
                tracing::warn!(
                    "[init] Failed to create agent for default workspace: {}. Workspace created without agent_id.",
                    e
                );
            }
        }
    }

    Ok(())
}
