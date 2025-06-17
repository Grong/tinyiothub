use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(m, "device_events",
            &[
                ("id", ColType::PkUuid),
                ("device_id", ColType::String),
                ("event_type", ColType::String),
                ("severity", ColType::String),
                ("payload", ColType::Json),
                ("timestamp", ColType::TimestampWithTimeZone),
            ],
            &[
                ("device_id", "devices"),
            ]
        ).await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "device_events").await
    }
}
