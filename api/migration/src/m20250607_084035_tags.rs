use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(m, "tags",
            &[
            ("id", ColType::PkAuto),
            ("name", ColType::StringNull),
            ("type", ColType::StringNull),
            ("tenant_id", ColType::StringNull),
            ("created_by", ColType::Integer)
            ],
            &[
            ]
        ).await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "tags").await
    }
}
