use loco_rs::schema::*;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        // Create thing_model table
        create_table(m, "thing_model",
            &[
                ("model_id", ColType::String),
                ("name", ColType::String),
                ("description", ColType::StringNull),
                ("version", ColType::String),
                ("schema_version", ColType::String),
                ("extensions", ColType::Json),
            ],
            &[]
        ).await?;

        // Create device table
        create_table(m, "device",
            &[
                ("device_id", ColType::String),
                ("tenant_id", ColType::String),
                ("model_id", ColType::String),
                ("name", ColType::String),
                ("config", ColType::Json),
                ("network_config", ColType::Json),
                ("security_config", ColType::Json),
            ],
            &[]
        ).await?;

        // Create model_property table
        create_table(m, "model_property",
            &[
                ("id", ColType::PkAuto),
                ("model_id", ColType::String),
                ("identifier", ColType::String),
                ("name", ColType::String),
                ("description", ColType::StringNull),
                ("data_type", ColType::String),
                ("access_mode", ColType::String),
                ("data_specs", ColType::Json),
            ],
            &[]
        ).await?;

        // Create model_service table
        create_table(m, "model_service",
            &[
                ("id", ColType::PkAuto),
                ("model_id", ColType::String),
                ("identifier", ColType::String),
                ("name", ColType::String),
                ("description", ColType::StringNull),
                ("call_type", ColType::String),
            ],
            &[]
        ).await?;

        // Create model_event table
        create_table(m, "model_event",
            &[
                ("id", ColType::PkAuto),
                ("model_id", ColType::String),
                ("identifier", ColType::String),
                ("name", ColType::String),
                ("description", ColType::StringNull),
                ("event_type", ColType::String),
                ("severity", ColType::String),
            ],
            &[]
        ).await?;

        // Create model_module table
        create_table(m, "model_module",
            &[
                ("id", ColType::PkAuto),
                ("model_id", ColType::String),
                ("name", ColType::String),
                ("description", ColType::StringNull),
            ],
            &[]
        ).await?;

        // Create module_item table
        create_table(m, "module_item",
            &[
                ("module_id", ColType::Integer),
                ("item_type", ColType::String),
                ("item_identifier", ColType::String),
            ],
            &[]
        ).await?;

        // Create service_param table
        create_table(m, "service_param",
            &[
                ("id", ColType::PkAuto),
                ("service_id", ColType::Integer),
                ("identifier", ColType::String),
                ("name", ColType::String),
                ("data_type", ColType::String),
                ("required", ColType::Boolean),
                ("data_specs", ColType::Json),
            ],
            &[]
        ).await?;

        // Create event_param table
        create_table(m, "event_param",
            &[
                ("id", ColType::PkAuto),
                ("event_id", ColType::Integer),
                ("identifier", ColType::String),
                ("name", ColType::String),
                ("data_type", ColType::String),
                ("data_specs", ColType::Json),
            ],
            &[]
        ).await?;

        // Create device_property_value table
        create_table(m, "device_property_value",
            &[
                ("device_id", ColType::String),
                ("id", ColType::String),
                ("identifier", ColType::String),
                ("display_name", ColType::String),
                ("data_type", ColType::String),
                ("access_mode", ColType::String),
                ("property_identifier", ColType::String),
                ("value", ColType::Json),
            ],
            &[]
        ).await?;

        // Create device_service_call table
        create_table(m, "device_service_call",
            &[
                ("id", ColType::PkAuto),
                ("device_id", ColType::String),
                ("identifier", ColType::String),
                ("input_parameters", ColType::Json),
                ("status", ColType::String),
                ("output_result", ColType::JsonNull),
            ],
            &[]
        ).await?;

        // Create device_event_record table
        create_table(m, "device_event_record",
            &[
                ("id", ColType::PkAuto),
                ("device_id", ColType::String),
                ("event_identifier", ColType::String),
                ("event_data", ColType::Json),
                ("occurred_at", ColType::TimestampWithTimeZone),
                ("severity", ColType::String),
            ],
            &[]
        ).await?;

        Ok(())
    }

    async fn down(&self, m: &SchemaManager) -> Result<(), DbErr> {
        // Drop tables in reverse order
        drop_table(m, "device_event_record").await?;
        drop_table(m, "device_service_call").await?;
        drop_table(m, "device_property_value").await?;
        drop_table(m, "event_param").await?;
        drop_table(m, "service_param").await?;
        drop_table(m, "module_item").await?;
        drop_table(m, "model_module").await?;
        drop_table(m, "model_event").await?;
        drop_table(m, "model_service").await?;
        drop_table(m, "model_property").await?;
        drop_table(m, "device").await?;
        drop_table(m, "thing_model").await?;

        Ok(())
    }
}
