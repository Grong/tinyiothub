use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "devices",
            &[
                ("id", ColType::PkAuto),
                ("name", ColType::String),
                ("description", ColType::StringNull),
                ("kind", ColType::StringNull),
                ("config", ColType::JsonNull),
                ("network_config", ColType::JsonNull),
                ("security_config", ColType::JsonNull),
                ("last_seen", ColType::TimestampWithTimeZoneNull),
                ("is_active", ColType::Boolean),
                ("status", ColType::Integer),
                ("extensions", ColType::JsonNull),
            ],
            &[
                ("tenant", "tenant_id"),
                ("user", "created_by"),
            ],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "devices").await
    }
}
