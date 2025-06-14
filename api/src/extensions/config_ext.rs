use serde_json::Value as JsonValue;

use crate::utils::settings::{Feature, SystemFeature,Settings};

pub trait ConfigExt {
    fn get_features(&self) -> Feature;
    fn get_system_features(&self) -> SystemFeature;
}

impl ConfigExt for loco_rs::config::Config {
    fn get_features(&self) -> Feature {
        let settings = self.settings.as_ref().unwrap_or(&JsonValue::Null);
        Settings::from_json_or_default(settings).features
    }

    fn get_system_features(&self) -> SystemFeature {
        let settings = self.settings.as_ref().unwrap_or(&JsonValue::Null);
        Settings::from_json_or_default(settings).system_features
    }
}
