use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "device_properties",
            &[
                ("id", ColType::PkUuid),
                ("device_id", ColType::String),
                ("identifier", ColType::String),
                ("display_name", ColType::String),
                ("value", ColType::String),
                ("data_type", ColType::Integer),
                ("status", ColType::Integer),
                ("data_specs", ColType::TextNull),
                ("description", ColType::StringNull),
            ],
            &[
                ("device_id", "devices"),
            ],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "device_properties").await
    }
}
