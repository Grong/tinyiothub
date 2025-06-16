#![allow(elided_lifetimes_in_paths)]
#![allow(clippy::wildcard_imports)]
pub use sea_orm_migration::prelude::*;
mod m20220101_000001_users;
mod m20250605_101407_apps;
mod m20250607_084035_tags;
mod m20250610_105058_create_thing_models;
pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_users::Migration),
            Box::new(m20250605_101407_apps::Migration),
            Box::new(m20250607_084035_tags::Migration),
            Box::new(m20250610_105058_create_thing_models::Migration),
        ]
    }
}
