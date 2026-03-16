use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::infrastructure::persistence::database::Database;

/// 设备数据类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Number,
    String,
    Boolean,
}

impl DataType {
    pub fn as_str(&self) -> &str {
        match self {
            DataType::Number => "number",
            DataType::String => "string",
            DataType::Boolean => "boolean",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "number" => DataType::Number,
            "boolean" => DataType::Boolean,
            _ => DataType::String,
        }
    }
}

/// 数据质量
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataQuality {
    Good,
    Bad,
    Uncertain,
}

impl DataQuality {
    pub fn as_str(&self) -> &str {
        match self {
            DataQuality::Good => "good",
            DataQuality::Bad => "bad",
            DataQuality::Uncertain => "uncertain",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bad" => DataQuality::Bad,
            "uncertain" => DataQuality::Uncertain,
            _ => DataQuality::Good,
        }
    }
}

/// 设备历史数据实体
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceData {
    pub id: String,
    pub device_id: String,
    pub property_name: String,
    pub property_value: String,
    pub property_type: String,
    pub unit: Option<String>,
    pub quality: String,
    pub timestamp: String,
    pub created_at: String,
}

/// 设备数据查询参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDataQuery {
    pub property_name: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

/// 创建设备数据请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CreateDeviceDataRequest {
    pub device_id: String,
    pub property_name: String,
    pub property_value: String,
    pub property_type: Option<String>,
    pub unit: Option<String>,
    pub quality: Option<String>,
    pub timestamp: Option<String>,
}

/// 批量创建设备数据请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BatchCreateDeviceDataRequest {
    pub device_id: String,
    pub data_points: Vec<DataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DataPoint {
    pub property_name: String,
    pub property_value: String,
    pub property_type: Option<String>,
    pub unit: Option<String>,
    pub quality: Option<String>,
    pub timestamp: Option<String>,
}

/// 设备数据统计
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceDataStats {
    pub id: String,
    pub device_id: String,
    pub property_name: String,
    pub count: i64,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
    pub avg_value: Option<f64>,
    pub last_updated: String,
}

/// 最新的设备数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LatestDeviceData {
    pub property_name: String,
    pub property_value: String,
    pub property_type: String,
    pub unit: Option<String>,
    pub quality: String,
    pub timestamp: String,
}

impl DeviceData {
    /// 根据 ID 查询
    pub async fn find_by_id(db: &Database, id: &str) -> Result<Option<DeviceData>, sqlx::Error> {
        let sql = format!("SELECT * FROM device_data WHERE id = '{}' LIMIT 1", id);
        
        let mut rows = db.query(&sql, |row| {
            Ok(DeviceData {
                id: row.try_get("id")?,
                device_id: row.try_get("device_id")?,
                property_name: row.try_get("property_name")?,
                property_value: row.try_get("property_value")?,
                property_type: row.try_get("property_type")?,
                unit: row.try_get("unit")?,
                quality: row.try_get("quality")?,
                timestamp: row.try_get("timestamp")?,
                created_at: row.try_get("created_at")?,
            })
        }).await?;
        
        Ok(rows.pop())
    }

    /// 根据设备 ID 查询历史数据
    pub async fn find_by_device(
        db: &Database,
        device_id: &str,
        query: &DeviceDataQuery,
    ) -> Result<Vec<DeviceData>, sqlx::Error> {
        let mut sql = format!("SELECT * FROM device_data WHERE device_id = '{}'", device_id);

        if let Some(ref property_name) = query.property_name {
            sql.push_str(&format!(" AND property_name = '{}'", property_name));
        }

        if let Some(ref start_time) = query.start_time {
            sql.push_str(&format!(" AND timestamp >= '{}'", start_time));
        }

        if let Some(ref end_time) = query.end_time {
            sql.push_str(&format!(" AND timestamp <= '{}'", end_time));
        }

        sql.push_str(" ORDER BY timestamp DESC");

        let page = query.page.unwrap_or(1);
        let page_size = query.page_size.unwrap_or(100);
        let offset = (page - 1) * page_size;
        sql.push_str(&format!(" LIMIT {} OFFSET {}", page_size, offset));

        db.query(&sql, |row| {
            Ok(DeviceData {
                id: row.try_get("id")?,
                device_id: row.try_get("device_id")?,
                property_name: row.try_get("property_name")?,
                property_value: row.try_get("property_value")?,
                property_type: row.try_get("property_type")?,
                unit: row.try_get("unit")?,
                quality: row.try_get("quality")?,
                timestamp: row.try_get("timestamp")?,
                created_at: row.try_get("created_at")?,
            })
        }).await
    }

    /// 获取设备的最新数据
    pub async fn find_latest(
        db: &Database,
        device_id: &str,
        property_name: Option<&str>,
    ) -> Result<Vec<LatestDeviceData>, sqlx::Error> {
        let sql = if let Some(prop) = property_name {
            format!(
                r#"SELECT d.property_name, d.property_value, d.property_type, d.unit, d.quality, d.timestamp
                   FROM device_data d
                   INNER JOIN (
                       SELECT property_name, MAX(timestamp) as max_ts
                       FROM device_data
                       WHERE device_id = '{}' AND property_name = '{}'
                       GROUP BY property_name
                   ) latest ON d.property_name = latest.property_name AND d.timestamp = latest.max_ts
                   WHERE d.device_id = '{}' AND d.property_name = '{}'"#,
                device_id, prop, device_id, prop
            )
        } else {
            format!(
                r#"SELECT d.property_name, d.property_value, d.property_type, d.unit, d.quality, d.timestamp
                   FROM device_data d
                   INNER JOIN (
                       SELECT property_name, MAX(timestamp) as max_ts
                       FROM device_data
                       WHERE device_id = '{}'
                       GROUP BY property_name
                   ) latest ON d.property_name = latest.property_name AND d.timestamp = latest.max_ts
                   WHERE d.device_id = '{}'"#,
                device_id, device_id
            )
        };

        db.query(&sql, |row| {
            Ok(LatestDeviceData {
                property_name: row.try_get("property_name")?,
                property_value: row.try_get("property_value")?,
                property_type: row.try_get("property_type")?,
                unit: row.try_get("unit")?,
                quality: row.try_get("quality")?,
                timestamp: row.try_get("timestamp")?,
            })
        }).await
    }

    /// 创建数据点
    pub async fn create(db: &Database, req: &CreateDeviceDataRequest) -> Result<DeviceData, sqlx::Error> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let timestamp = req.timestamp.as_ref().unwrap_or(&now).clone();
        let property_type = req.property_type.as_deref().unwrap_or("string");
        let quality = req.quality.as_deref().unwrap_or("good");
        
        let sql = format!(
            r#"INSERT INTO device_data (id, device_id, property_name, property_value, property_type, unit, quality, timestamp, created_at)
               VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}')"#,
            id,
            req.device_id,
            req.property_name,
            req.property_value,
            property_type,
            req.unit.as_deref().unwrap_or(""),
            quality,
            timestamp,
            now
        );

        db.execute(&sql).await?;

        Self::find_by_id(db, &id).await?.ok_or(sqlx::Error::RowNotFound)
    }

    /// 批量创建数据点
    pub async fn create_batch(db: &Database, device_id: &str, data_points: &[DataPoint]) -> Result<Vec<DeviceData>, sqlx::Error> {
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let mut created = Vec::new();

        for point in data_points {
            let id = uuid::Uuid::new_v4().to_string();
            let timestamp = point.timestamp.as_ref().unwrap_or(&now).clone();
            let property_type = point.property_type.as_deref().unwrap_or("string");
            let quality = point.quality.as_deref().unwrap_or("good");

            let sql = format!(
                r#"INSERT INTO device_data (id, device_id, property_name, property_value, property_type, unit, quality, timestamp, created_at)
                   VALUES ('{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}', '{}')"#,
                id,
                device_id,
                point.property_name,
                point.property_value,
                property_type,
                point.unit.as_deref().unwrap_or(""),
                quality,
                timestamp,
                now
            );

            db.execute(&sql).await?;
            
            if let Some(data) = Self::find_by_id(db, &id).await? {
                created.push(data);
            }
        }

        Ok(created)
    }

    /// 删除历史数据（用于数据清理）
    pub async fn delete_old(db: &Database, days: i64) -> Result<u64, sqlx::Error> {
        let sql = format!(
            "DELETE FROM device_data WHERE timestamp < datetime('now', '-{} days')",
            days
        );
        
        let result = db.execute(&sql).await?;
        Ok(result)
    }
}
