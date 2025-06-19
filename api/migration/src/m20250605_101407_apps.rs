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
                ("description", ColType::TextNull),
                ("llms", ColType::TextNull),
                ("status", ColType::String),
                ("is_public", ColType::Boolean),
                ("is_demo", ColType::Boolean),
                ("mode", ColType::StringNull),
                ("icon_type", ColType::StringNull),
                ("icon", ColType::StringNull),
                ("icon_background", ColType::StringNull),
                ("enable_site", ColType::Boolean),
                ("enable_api", ColType::Boolean),
                ("api_rpm", ColType::Integer),
                ("api_rph", ColType::Integer),
                ("tracing", ColType::TextNull),
                ("site", ColType::JsonNull),
                ("api_base_url", ColType::StringNull),
                ("tags", ColType::JsonNull),
                ("access_mode", ColType::StringNull),
            ],
            &[
                ("tenant", "tenant_id"),
                ("user", "created_by"),
                ("user", "updated_by"),
            ],
        )
        .await
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        drop_table(m, "apps").await
    }
}
