#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;

mod m20220101_000001_users;
mod m20250605_101407_apps;
mod m20250607_084035_tags;
mod m20250617_103012_tag_bindings;
mod m20250617_103958_devices;
mod m20250617_104259_device_properties;
mod m20250617_104446_device_events;
mod m20250617_104607_device_service_calls;
mod m20250617_104839_device_templates;
mod m20250618_021341_tenants;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20250605_101407_apps::Migration),
            Box::new(m20250607_084035_tags::Migration),
            Box::new(m20250617_103012_tag_bindings::Migration),
            Box::new(m20250617_103958_devices::Migration),
            Box::new(m20250617_104259_device_properties::Migration),
            Box::new(m20250617_104446_device_events::Migration),
            Box::new(m20250617_104607_device_service_calls::Migration),
            Box::new(m20250617_104839_device_templates::Migration),
            Box::new(m20250618_021341_tenants::Migration),
        ]
    }
}