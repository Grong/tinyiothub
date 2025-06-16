use crate::models::device::{self, DeviceStatus};
use crate::models::device_property::{PropertyDataType, PropertyStatus};
use crate::prelude::*;
use serde_json::Value;

pub struct DevicePropertyService;

impl DevicePropertyService {
    /// 更新属性值并计算状态
    pub async fn update_property(
        db: &DatabaseConnection,
        device_id: &str,
        property_name: &str,
        value: Value,
    ) -> Result<device::Model> {
        let device = device::Entity::find_by_id(device_id)
            .one(db)
            .await?
            .ok_or_else(|| Error::NotFound)?;

        let prop =
            DevicePropertyModel::find_prop_by_device_id(db, device_id, property_name).await?;
        let mut prop_active = prop.into_active_model();
        prop_active.value = Set(value.to_string());
        prop_active.update(db).await?;

        let overall_status = DeviceStatus::Normal;

        let mut active_device = device.into_active_model();
        active_device.updated_at = Set(chrono::Utc::now().fixed_offset());
        active_device.status = Set(overall_status);

        let updated = active_device.update(db).await?;
        Ok(updated)
    }

    /// 根据属性类型和值计算状态
    fn calculate_property_status(property: &DevicePropertyModel, value: &Value) -> PropertyStatus {
        match property.data_type.clone() {
            PropertyDataType::Number
            | PropertyDataType::Float
            | PropertyDataType::Double
            | PropertyDataType::Int => {
                if let Some(num) = value.as_f64() {
                    // 检查阈值
                    if let Some(thresholds) = property.get_thresholds() {
                        let high = thresholds.high;
                        let low = thresholds.low;

                        if num > high {
                            return PropertyStatus::Alarm;
                        } else if num < low {
                            return PropertyStatus::Alarm;
                        }
                    }

                    // 检查是否在正常范围内
                    if let Some(range) = property.get_valid_range() {
                        let min = range.min;
                        let max = range.max;

                        if num < min || num > max {
                            return PropertyStatus::Alarm;
                        }
                    }
                }
                PropertyStatus::Normal
            }
            PropertyDataType::Boolean => {
                if let Some(b) = value.as_bool() {
                    if let Some(expected) = property.get_expected_value() {
                        if expected != b {
                            return PropertyStatus::Alarm;
                        }
                    }
                }
                PropertyStatus::Normal
            }
            PropertyDataType::Enum => {
                if let Some(s) = value.as_str() {
                    if let Some(options) = property.get_options() {
                        if !options.iter().any(|opt| opt == s) {
                            return PropertyStatus::Alarm;
                        }
                    }
                }
                PropertyStatus::Normal
            }
            PropertyDataType::Image | PropertyDataType::Video => PropertyStatus::Normal,
            _ => PropertyStatus::Normal,
        }
    }

    /// 设置属性阈值
    pub async fn set_property_thresholds(
        db: &DatabaseConnection,
        device_id: &str,
        property_name: &str,
        thresholds: Value,
    ) -> Result<DevicePropertyModel> {
        let prop =
            DevicePropertyModel::find_prop_by_device_id(db, device_id, property_name).await?;
        let status = Self::calculate_property_status(&prop, &thresholds);
        let mut prop_active = prop.into_active_model();
        prop_active.status = Set(status);
        let updated = prop_active.update(db).await?;
        Ok(updated)
    }
}
