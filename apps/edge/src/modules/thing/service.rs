use std::sync::Arc;

use crate::shared::error::{EdgeError, EdgeResult};
use tinyiothub_core::models::thing::{CreateThingRequest, Thing};
use tinyiothub_storage::Db;
use tinyiothub_storage::thing::ThingCriteria;

pub struct ThingService {
    db: Arc<Db>,
}

impl ThingService {
    pub fn new(db: Arc<Db>) -> Arc<Self> {
        Arc::new(Self { db })
    }

    pub async fn get_thing(&self, id: &str) -> EdgeResult<Thing> {
        self.db
            .find_thing_by_id(None, id)
            .await?
            .ok_or_else(|| EdgeError::Internal(format!("thing not found: {}", id)))
    }

    pub async fn list_things(&self, driver_name: Option<&str>) -> EdgeResult<Vec<Thing>> {
        let criteria = if let Some(dn) = driver_name {
            ThingCriteria {
                driver_name: Some(dn.to_string()),
                ..Default::default()
            }
        } else {
            ThingCriteria::default()
        };
        Ok(self.db.find_things(None, &criteria).await?)
    }

    pub async fn sync_from_cloud(&self, cloud_things: &[CreateThingRequest]) -> EdgeResult<Vec<Thing>> {
        if cloud_things.is_empty() {
            return Ok(Vec::new());
        }
        Ok(self.db.create_things_batch(None, cloud_things).await?)
    }

    pub async fn get_driver_for_thing(&self, thing_id: &str) -> EdgeResult<String> {
        let thing = self.get_thing(thing_id).await?;
        thing
            .driver_name
            .ok_or_else(|| EdgeError::Internal("thing has no driver configured".into()))
    }
}
