// TODO(D6): edge 自建本地表（ensure_things_table 等）是暂留形态——后期 edge
// 直接只复用 crates/db 的 baseline（删库重建，另立项），届时本文件的可表创建逻辑
// 随 Db::connect 统一迁移而删除。
use super::error::EdgeResult;
use std::sync::Arc;
use tinyiothub_storage::{DatabaseConfig, Db, create_pool_without_migrations};

/// Initialize the SQLite database and return an Arc<Db>.
pub async fn init_database(db_path: &str) -> EdgeResult<Arc<Db>> {
    let config = DatabaseConfig::from_file_path(db_path);
    let pool = create_pool_without_migrations(&config).await?;
    let db = Db::new(pool);

    // Ensure core tables exist (edge gateway needs things locally)
    db.ensure_things_table().await?;

    Ok(Arc::new(db))
}
