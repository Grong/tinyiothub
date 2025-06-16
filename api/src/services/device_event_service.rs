use crate::models::{
    device,
    device_event::{self, EventType},
};
use loco_rs::prelude::*;
use sea_orm::{entity::prelude::*, ActiveValue::Set, IntoActiveModel};
use serde_json::Value;

pub struct DeviceEventService;

impl DeviceEventService {
    /// 记录设备事件
    pub async fn record_event(
        db: &DatabaseConnection,
        device_id: &str,
        event_type: &str,
        payload: Value,
    ) -> Result<device_event::Model> {
        // 更新设备最后在线时间
        let device = device::Entity::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let mut active_device = device.into_active_model();
        active_device.last_seen = Set(Some(chrono::Utc::now().fixed_offset()));
        active_device.update(db).await?;

        // 创建事件记录
        let event = device_event::ActiveModel {
            device_id: Set(device_id.to_string()),
            event_type: Set(EventType::from_str(event_type).unwrap()),
            payload: Set(payload),
            ..Default::default()
        }
        .insert(db)
        .await?;

        Ok(event)
    }
}
