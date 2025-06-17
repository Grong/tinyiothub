use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        create_table(m, "tag_bindings",
            &[
            
            ("id", ColType::PkAuto),
            
            ("tenant_id", ColType::StringNull),
            ("tag_id", ColType::Integer),
            ("target_id", ColType::Integer),
            ("created_by", ColType::IntegerNull),
            ],
            &[
                ("tag_id", "tags"),
                ("target_id", "apps"),
            ]
        ).await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "tag_bindings").await
    }
}
