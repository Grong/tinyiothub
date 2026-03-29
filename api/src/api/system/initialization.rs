use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{
    dto::{
        entity::user::{CreateUserRequest, User},
        response::ApiResponse,
    },
    shared::app_state::AppState,
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
    match User::find_all(state.database(), &Default::default()).await {
        Ok(users) if !users.is_empty() => {
            return ApiResponse::success(InitializeResponse {
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
            return ApiResponse::error("系统初始化失败".to_string());
        }
    }

    // 验证输入
    if request.admin_username.trim().is_empty() {
        return ApiResponse::error("管理员用户名不能为空".to_string());
    }

    if request.admin_password.len() < 6 {
        return ApiResponse::error("管理员密码长度不能少于6位".to_string());
    }

    // 创建管理员用户
    let create_request = CreateUserRequest {
        username: request.admin_username.clone(),
        password: request.admin_password,
        phone: None,
        email: request.admin_email,
        display_name: None,
        is_enabled: Some(true), // 启用状态
        parent_id: None,
    };

    match User::create(state.database(), &create_request).await {
        Ok(admin_user) => {
            tracing::info!("System initialized with admin user: {}", admin_user.get_display_name());

            ApiResponse::success(InitializeResponse {
                success: true,
                message: "系统初始化成功".to_string(),
                admin_user_id: Some(admin_user.id),
            })
        }
        Err(e) => {
            tracing::error!("Failed to create admin user: {}", e);
            ApiResponse::error("创建管理员用户失败".to_string())
        }
    }
}

/// 检查系统是否需要初始化
pub async fn check_system_initialization(state: &AppState) -> Result<bool, sqlx::Error> {
    let users = User::find_all(state.database(), &Default::default()).await?;
    Ok(users.is_empty())
}

/// 自动创建默认管理员用户（如果不存在）
pub async fn ensure_default_admin_user(state: &AppState) -> Result<(), sqlx::Error> {
    // 先查找 admin 用户是否已存在
    let admin_user = User::find_by_username(state.database(), "admin").await?;

    if let Some(user) = admin_user {
        // admin 用户已存在，检查密码哈希是否是迁移脚本里的假哈希
        if user.password_hash == "FIX_ME_admin_hash" || user.password_hash == "hashed_admin123" {
            tracing::info!("[init] Admin user has invalid password hash from migration, fixing...");
            match User::update_password(state.database(), &user.id, "admin123").await {
                Ok(_) => {
                    tracing::info!("[init] Admin password fixed successfully");
                }
                Err(e) => {
                    tracing::error!("[init] Failed to fix admin password: {}", e);
                    return Err(e);
                }
            }
        }
        // 密码哈希正确则不修改
    } else {
        // 创建默认管理员用户
        tracing::info!("[init] No admin user found, creating default admin...");
        let create_request = CreateUserRequest {
            username: "admin".to_string(),
            password: "admin123".to_string(), // 默认密码，生产环境应该要求用户修改
            phone: None,
            email: Some("admin@tinyiothub.local".to_string()),
            display_name: Some("Administrator".to_string()),
            is_enabled: Some(true),
            parent_id: None,
        };

        match User::create(state.database(), &create_request).await {
            Ok(admin_user) => {
                tracing::info!(
                    "Created default admin user: {} (ID: {})",
                    admin_user.get_display_name(),
                    admin_user.id
                );
                tracing::warn!(
                    "Default admin password is 'admin123', please change it immediately!"
                );
            }
            Err(e) => {
                tracing::error!("Failed to create default admin user: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
