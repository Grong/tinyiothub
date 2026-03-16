//! Gateway Entity
//! 网关实体定义

use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::infrastructure::persistence::database::Database;

/// 网关状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GatewayStatus {
    Online,
    Offline,
}

impl GatewayStatus {
    pub fn as_str(&self) -> &str {
        match self {
            GatewayStatus::Online => "online",
            GatewayStatus::Offline => "offline",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "online" => GatewayStatus::Online,
            _ => GatewayStatus::Offline,
        }
    }
}

/// 网关实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Gateway {
    pub id: String,
    pub name: String,
    pub token: Option<String>,
    pub token_expires_at: Option<String>,
    pub status: String,
    pub gateway_type: Option<String>,
    pub firmware_version: Option<String>,
    pub last_seen: Option<String>,
    pub api_key: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 创建网关请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateGatewayRequest {
    pub name: String,
    pub api_key: String,
    pub gateway_type: Option<String>,
    pub firmware_version: Option<String>,
}

/// 更新网关请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UpdateGatewayRequest {
    pub name: Option<String>,
    pub status: Option<String>,
    pub gateway_type: Option<String>,
    pub firmware_version: Option<String>,
}

/// 网关设备关联
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GatewayDevice {
    pub id: String,
    pub gateway_id: String,
    pub device_id: String,
    pub created_at: String,
}

/// 设备列表上报请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceListReport {
    pub devices: Vec<DeviceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceInfo {
    pub device_id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub device_type: String,
    pub protocol: String,
    pub online: bool,
    pub properties: Option<serde_json::Value>,
}

impl Gateway {
    /// 根据 ID 查询
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<Gateway>, sqlx::Error> {
        let sql = format!("SELECT * FROM gateways WHERE id = '{}' LIMIT 1", id);
        
        let mut rows = db.query(&sql, |row| {
            Ok(Gateway {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                token: row.try_get("token")?,
                token_expires_at: row.try_get("token_expires_at")?,
                status: row.try_get("status")?,
                gateway_type: row.try_get("gateway_type")?,
                firmware_version: row.try_get("firmware_version")?,
                last_seen: row.try_get("last_seen")?,
                api_key: row.try_get("api_key")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        }).await?;
        
        Ok(rows.pop())
    }

    /// 根据 API Key 查询
    pub async fn find_by_api_key(db: &Database, api_key: &str) -> Result<Option<Gateway>, sqlx::Error> {
        let sql = format!("SELECT * FROM gateways WHERE api_key = '{}' LIMIT 1", api_key);
        
        let mut rows = db.query(&sql, |row| {
            Ok(Gateway {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                token: row.try_get("token")?,
                token_expires_at: row.try_get("token_expires_at")?,
                status: row.try_get("status")?,
                gateway_type: row.try_get("gateway_type")?,
                firmware_version: row.try_get("firmware_version")?,
                last_seen: row.try_get("last_seen")?,
                api_key: row.try_get("api_key")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        }).await?;
        
        Ok(rows.pop())
    }

    /// 查询所有网关
    pub async fn find_all(db: &Database, status: Option<&str>) -> Result<Vec<Gateway>, sqlx::Error> {
        let mut sql = String::from("SELECT * FROM gateways");
        
        if let Some(s) = status {
            sql.push_str(&format!(" WHERE status = '{}'", s));
        }
        
        sql.push_str(" ORDER BY created_at DESC");
        
        db.query(&sql, |row| {
            Ok(Gateway {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                token: row.try_get("token")?,
                token_expires_at: row.try_get("token_expires_at")?,
                status: row.try_get("status")?,
                gateway_type: row.try_get("gateway_type")?,
                firmware_version: row.try_get("firmware_version")?,
                last_seen: row.try_get("last_seen")?,
                api_key: row.try_get("api_key")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        }).await
    }

    /// 创建网关
    pub async fn create(db: &Database, req: &CreateGatewayRequest) -> Result<Gateway, sqlx::Error> {
        let id = format!("gw-{}", uuid::Uuid::new_v4().to_string()[..8].to_string());
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let sql = format!(
            r#"INSERT INTO gateways (id, name, api_key, gateway_type, firmware_version, status, created_at, updated_at)
               VALUES ('{}', '{}', '{}', '{}', '{}', 'offline', '{}', '{}')"#,
            id,
            req.name,
            req.api_key,
            req.gateway_type.as_deref().unwrap_or(""),
            req.firmware_version.as_deref().unwrap_or(""),
            now,
            now
        );
        
        db.execute(&sql).await?;
        
        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新网关
    pub async fn update(db: &Database, id: &str, req: &UpdateGatewayRequest) -> Result<Gateway, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let mut updates = vec![format!("updated_at = '{}'", now)];
        
        if let Some(ref name) = req.name {
            updates.push(format!("name = '{}'", name));
        }
        if let Some(ref status) = req.status {
            updates.push(format!("status = '{}'", status));
        }
        if let Some(ref gateway_type) = req.gateway_type {
            updates.push(format!("gateway_type = '{}'", gateway_type));
        }
        
        let sql = format!("UPDATE gateways SET {} WHERE id = '{}'", updates.join(", "), id);
        let _ = db.execute(&sql).await;
        
        Self::find_by_id(db, id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 更新 Token
    pub async fn update_token(db: &Database, id: &str, token: &str, expires_at: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let sql = format!(
            "UPDATE gateways SET token = '{}', token_expires_at = '{}', updated_at = '{}' WHERE id = '{}'",
            token, expires_at, now, id
        );
        
        db.execute(&sql).await?;
        Ok(())
    }

    /// 更新在线状态
    pub async fn update_status(db: &Database, id: &str, status: &str) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let sql = format!(
            "UPDATE gateways SET status = '{}', last_seen = '{}', updated_at = '{}' WHERE id = '{}'",
            status, now, now, id
        );
        
        db.execute(&sql).await?;
        Ok(())
    }

    /// 删除网关
    pub async fn delete(db: &Database, id: &str) -> Result<(), sqlx::Error> {
        let sql = format!("DELETE FROM gateways WHERE id = '{}'", id);
        db.execute(&sql).await?;
        Ok(())
    }
}

impl GatewayDevice {
    /// 绑定设备到网关
    pub async fn bind_device(db: &Database, gateway_id: &str, device_id: &str) -> Result<GatewayDevice, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        
        let sql = format!(
            "INSERT INTO gateway_devices (id, gateway_id, device_id, created_at) VALUES ('{}', '{}', '{}', '{}')",
            id, gateway_id, device_id, now
        );
        
        db.execute(&sql).await?;
        
        let sql = format!("SELECT * FROM gateway_devices WHERE id = '{}' LIMIT 1", id);
        let mut rows = db.query(&sql, |row| {
            Ok(GatewayDevice {
                id: row.try_get("id")?,
                gateway_id: row.try_get("gateway_id")?,
                device_id: row.try_get("device_id")?,
                created_at: row.try_get("created_at")?,
            })
        }).await?;
        
        rows.pop().ok_or(sqlx::Error::RowNotFound)
    }

    /// 解除设备绑定
    pub async fn unbind_device(db: &Database, gateway_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        let sql = format!(
            "DELETE FROM gateway_devices WHERE gateway_id = '{}' AND device_id = '{}'",
            gateway_id, device_id
        );
        db.execute(&sql).await?;
        Ok(())
    }

    /// 获取网关下的所有设备
    pub async fn get_gateway_devices(db: &Database, gateway_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let sql = format!(
            "SELECT device_id FROM gateway_devices WHERE gateway_id = '{}'",
            gateway_id
        );
        
        db.query(&sql, |row| {
            Ok(row.try_get::<String, _>("device_id")?)
        }).await
    }
}
