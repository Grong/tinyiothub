use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(
            m,
            "apps",
            &[
                ("id", ColType::PkAuto),
                ("name", ColType::StringNull),
                ("des", ColType::TextNull),
                ("llms", ColType::TextNull),
                ("created_by", ColType::Integer),
                ("updated_by", ColType::Integer),
                ("status", ColType::String),
                ("is_public", ColType::Boolean),
                ("is_demo", ColType::Boolean),
                ("mode", ColType::StringNull),
                ("icon_type", ColType::StringNull),
                ("icon", ColType::StringNull),
            ],
            &[],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "apps").await
    }
}
