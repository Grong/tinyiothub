use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use sqlx::{
    SqlSafeStr, SqlitePool,
    migrate::{Migration, MigrationType, Migrator},
};
#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;
#[cfg(test)]
use tokio::sync::Notify;

use crate::{
    database::Db,
    error::{DbError, Result},
};

#[derive(Debug)]
pub(crate) enum WorkspaceStoreState {
    Empty,
    Available(SqlitePool),
    Unavailable,
    Closed,
}

pub(crate) type WorkspaceStoreEntry = Arc<Mutex<WorkspaceStoreState>>;
pub(crate) type WorkspaceStores = Arc<Mutex<HashMap<String, WorkspaceStoreEntry>>>;

#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct WorkspaceStoreTestHooks {
    pub gated_workspace_id: String,
    pub open_started: Notify,
    pub open_release: Notify,
    pub fail_open: AtomicBool,
}

pub(crate) fn default_workspace_store_root() -> PathBuf {
    std::env::var_os("TINYIOTHUB__PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .join("data/agents")
}

fn harness_migrator() -> Migrator {
    Migrator::with_migrations(vec![
        Migration::new(
            20261103000001,
            "harness_workspace".into(),
            MigrationType::Simple,
            include_str!("../migrations/20261103000001_harness_workspace.sql").into_sql_str(),
            false,
        ),
        crate::harness_event::migration(),
        crate::harness_effect::migration(),
    ])
}

fn validate_workspace_id(workspace_id: &str) -> Result<()> {
    if workspace_id.is_empty()
        || workspace_id == "."
        || workspace_id == ".."
        || workspace_id.contains('/')
        || workspace_id.contains('\\')
    {
        return Err(DbError::Validation {
            message: "workspace ID must be a single path component".to_string(),
        });
    }
    Ok(())
}

async fn open_workspace_store(db: &Db, _workspace_id: &str, path: &Path) -> Result<SqlitePool> {
    #[cfg(test)]
    if let Some(hooks) = &db.workspace_store_test_hooks {
        if hooks.gated_workspace_id == _workspace_id {
            hooks.open_started.notify_one();
            hooks.open_release.notified().await;
        }
        if hooks.fail_open.swap(false, Ordering::SeqCst) {
            return Err(DbError::Internal("injected workspace open failure".to_string()));
        }
    }

    let parent = path
        .parent()
        .ok_or_else(|| DbError::Internal("workspace store path has no parent".to_string()))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| DbError::Internal(format!("failed to create workspace store directory: {error}")))?;
    let pool = crate::pool::workspace_pool_options(&db.workspace_store_config)
        .connect_with(crate::pool::workspace_connect_options(path))
        .await?;
    if let Err(error) = harness_migrator().run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    Ok(pool)
}

impl Db {
    async fn ensure_registered_workspace(&self, workspace_id: &str) -> Result<()> {
        validate_workspace_id(workspace_id)?;
        if crate::workspace::workspace_exists_by_id(self.pool(), workspace_id).await? {
            Ok(())
        } else {
            Err(DbError::NotFound {
                id: workspace_id.to_string(),
            })
        }
    }

    async fn workspace_store_entry(&self, workspace_id: &str) -> WorkspaceStoreEntry {
        let mut stores = self.workspace_stores.lock().await;
        stores
            .entry(workspace_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(WorkspaceStoreState::Empty)))
            .clone()
    }

    async fn open_workspace_store_entry(
        &self,
        workspace_id: &str,
        state: &mut WorkspaceStoreState,
    ) -> Result<SqlitePool> {
        let path = self.workspace_store_root.join(workspace_id).join("harness/harness.db");
        match open_workspace_store(self, workspace_id, &path).await {
            Ok(pool) => {
                *state = WorkspaceStoreState::Available(pool.clone());
                Ok(pool)
            }
            Err(error) => {
                if matches!(state, WorkspaceStoreState::Empty) {
                    *state = WorkspaceStoreState::Unavailable;
                }
                tracing::error!(workspace_id, error = %error, "workspace harness store open failed");
                Err(error)
            }
        }
    }

    pub async fn create_workspace_store(&self, workspace_id: &str) -> Result<()> {
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        match &*state {
            WorkspaceStoreState::Empty => {
                drop(state);
                self.workspace_pool(workspace_id).await.map(|_| ())
            }
            WorkspaceStoreState::Available(_) => self.ensure_registered_workspace(workspace_id).await,
            WorkspaceStoreState::Closed | WorkspaceStoreState::Unavailable => {
                self.ensure_registered_workspace(workspace_id).await?;
                self.open_workspace_store_entry(workspace_id, &mut state)
                    .await
                    .map(|_| ())
            }
        }
    }

    pub(crate) async fn workspace_pool(&self, workspace_id: &str) -> Result<SqlitePool> {
        self.ensure_registered_workspace(workspace_id).await?;
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        match &*state {
            WorkspaceStoreState::Available(pool) => return Ok(pool.clone()),
            WorkspaceStoreState::Unavailable => {
                return Err(DbError::Internal(format!(
                    "workspace harness store is unavailable: {workspace_id}"
                )));
            }
            WorkspaceStoreState::Closed => {
                return Err(DbError::Internal(format!(
                    "workspace harness store is closed: {workspace_id}"
                )));
            }
            WorkspaceStoreState::Empty => {}
        }
        self.open_workspace_store_entry(workspace_id, &mut state).await
    }

    #[cfg(any(test, feature = "testing"))]
    pub async fn workspace_pool_for_testing(&self, workspace_id: &str) -> Result<SqlitePool> {
        self.workspace_pool(workspace_id).await
    }

    pub async fn close_workspace_store(&self, workspace_id: &str) -> Result<()> {
        self.ensure_registered_workspace(workspace_id).await?;
        let entry = self.workspace_store_entry(workspace_id).await;
        let mut state = entry.lock().await;
        let previous = std::mem::replace(&mut *state, WorkspaceStoreState::Closed);
        if let WorkspaceStoreState::Available(pool) = previous {
            pool.close().await;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::{Arc, atomic::Ordering},
        time::Duration,
    };

    use sqlx::SqlitePool;
    use tokio::time::timeout;

    use super::WorkspaceStoreTestHooks;
    use crate::Db;

    struct HarnessWorkspaceFixture {
        root: PathBuf,
        db: Db,
    }

    impl HarnessWorkspaceFixture {
        async fn new(workspace_ids: &[&str]) -> Self {
            let root = std::env::temp_dir().join(format!("tinyiothub-harness-workspace-{}", uuid::Uuid::new_v4()));
            let pool = crate::test_helpers::test_pool().await;
            let mut db = Db::new(pool);
            db.workspace_store_root = root.join("data/agents");
            crate::seed::seed_system(&db).await.expect("seed platform registry");
            for workspace_id in workspace_ids {
                sqlx::query(
                    "INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 'tenant-default-001', 'now', 'now')",
                )
                .bind(workspace_id)
                .execute(db.pool())
                .await
                .expect("insert workspace");
            }
            Self { root, db }
        }

        fn database_path(&self, workspace_id: &str) -> PathBuf {
            self.root
                .join("data/agents")
                .join(workspace_id)
                .join("harness/harness.db")
        }

        async fn close(self) {
            self.db.pool().close().await;
            let _ = tokio::fs::remove_dir_all(&self.root).await;
        }
    }

    async fn create_conflicting_workspace_database(path: &Path) {
        tokio::fs::create_dir_all(path.parent().expect("database parent"))
            .await
            .expect("create harness directory");
        let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await
            .expect("open conflicting database");
        sqlx::query(
            "CREATE TABLE _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("create migration table");
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (20261103000001, 'harness_workspace', 1, X'00', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert conflicting migration checksum");
        pool.close().await;
    }

    async fn create_probe_table(pool: &SqlitePool) {
        sqlx::query("CREATE TABLE IF NOT EXISTS harness_workspace (id INTEGER PRIMARY KEY)")
            .execute(pool)
            .await
            .expect("create lifecycle probe table");
    }

    #[tokio::test]
    async fn harness_workspace_create_reopen_and_pragmas() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-create"]).await;
        let path = fixture.database_path("ws-create");

        fixture
            .db
            .create_workspace_store("ws-create")
            .await
            .expect("create workspace store");
        assert_eq!(path, fixture.root.join("data/agents/ws-create/harness/harness.db"));
        assert!(path.is_file());

        let pool = fixture.db.workspace_pool("ws-create").await.expect("workspace pool");
        let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("journal mode");
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .expect("foreign keys");
        assert_eq!(journal_mode, "wal");
        assert_eq!(foreign_keys, 1);
        let migration_version: i64 =
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE version = 20261103000001")
                .fetch_one(&pool)
                .await
                .expect("harness migration is recorded");
        assert_eq!(migration_version, 20261103000001);
        create_probe_table(&pool).await;
        sqlx::query("INSERT INTO harness_workspace (id) VALUES (1)")
            .execute(&pool)
            .await
            .expect("write workspace store");

        fixture
            .db
            .close_workspace_store("ws-create")
            .await
            .expect("close workspace store");
        let error = fixture
            .db
            .workspace_pool("ws-create")
            .await
            .expect_err("closed store must not reopen implicitly");
        assert!(error.to_string().contains("closed"), "unexpected error: {error}");

        fixture
            .db
            .create_workspace_store("ws-create")
            .await
            .expect("explicitly reopen workspace store");
        let reopened = fixture.db.workspace_pool("ws-create").await.expect("reopened pool");
        let id: i64 = sqlx::query_scalar("SELECT id FROM harness_workspace")
            .fetch_one(&reopened)
            .await
            .expect("read reopened workspace store");
        assert_eq!(id, 1);

        fixture
            .db
            .close_workspace_store("ws-create")
            .await
            .expect("close reopened store");
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_workspace_rejects_unknown_and_traversing_ids() {
        let fixture = HarnessWorkspaceFixture::new(&[]).await;

        fixture
            .db
            .workspace_pool("ws-unknown")
            .await
            .expect_err("unknown workspace must be rejected");
        fixture
            .db
            .create_workspace_store("../outside")
            .await
            .expect_err("path traversal must be rejected");
        assert!(!fixture.root.join("data/outside").exists());

        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_workspace_migration_failure_is_isolated() {
        let fixture = HarnessWorkspaceFixture::new(&["ws-bad", "ws-good"]).await;
        create_conflicting_workspace_database(&fixture.database_path("ws-bad")).await;

        fixture
            .db
            .create_workspace_store("ws-bad")
            .await
            .expect_err("conflicting migration must fail");
        let error = fixture
            .db
            .workspace_pool("ws-bad")
            .await
            .expect_err("failed workspace must stay unavailable");
        assert!(error.to_string().contains("unavailable"), "unexpected error: {error}");

        fixture
            .db
            .create_workspace_store("ws-good")
            .await
            .expect("another workspace must still migrate");
        let pool = fixture.db.workspace_pool("ws-good").await.expect("good workspace pool");
        create_probe_table(&pool).await;
        sqlx::query("INSERT INTO harness_workspace (id) VALUES (1)")
            .execute(&pool)
            .await
            .expect("another workspace must still write");
        let id: i64 = sqlx::query_scalar("SELECT id FROM harness_workspace")
            .fetch_one(&pool)
            .await
            .expect("another workspace must still read");
        assert_eq!(id, 1);

        fixture
            .db
            .close_workspace_store("ws-good")
            .await
            .expect("close good store");
        fixture.close().await;
    }

    #[tokio::test]
    async fn harness_workspace_slow_open_does_not_block_another_workspace() {
        let mut fixture = HarnessWorkspaceFixture::new(&["ws-slow", "ws-fast"]).await;
        let hooks = Arc::new(WorkspaceStoreTestHooks {
            gated_workspace_id: "ws-slow".to_string(),
            ..Default::default()
        });
        fixture.db.workspace_store_test_hooks = Some(hooks.clone());

        let slow_db = fixture.db.clone();
        let slow = tokio::spawn(async move { slow_db.create_workspace_store("ws-slow").await });
        hooks.open_started.notified().await;

        let fast_db = fixture.db.clone();
        let fast_result = timeout(Duration::from_secs(2), async move {
            fast_db.create_workspace_store("ws-fast").await?;
            let pool = fast_db.workspace_pool("ws-fast").await?;
            create_probe_table(&pool).await;
            sqlx::query("INSERT INTO harness_workspace (id) VALUES (1)")
                .execute(&pool)
                .await
                .expect("write fast workspace");
            let id: i64 = sqlx::query_scalar("SELECT id FROM harness_workspace")
                .fetch_one(&pool)
                .await
                .expect("read fast workspace");
            assert_eq!(id, 1);
            Ok::<(), crate::DbError>(())
        })
        .await;

        hooks.open_release.notify_one();
        slow.await.expect("slow task").expect("slow workspace open");
        fixture
            .db
            .close_workspace_store("ws-slow")
            .await
            .expect("close slow store");
        if matches!(fast_result, Ok(Ok(()))) {
            fixture
                .db
                .close_workspace_store("ws-fast")
                .await
                .expect("close fast store");
        }
        fixture.close().await;

        fast_result
            .expect("fast workspace was blocked by slow workspace open")
            .expect("fast workspace lifecycle");
    }

    #[tokio::test]
    async fn harness_workspace_failed_reopen_preserves_closed_state() {
        let mut fixture = HarnessWorkspaceFixture::new(&["ws-reopen"]).await;
        let hooks = Arc::new(WorkspaceStoreTestHooks::default());
        fixture.db.workspace_store_test_hooks = Some(hooks.clone());
        fixture
            .db
            .create_workspace_store("ws-reopen")
            .await
            .expect("create store");
        fixture
            .db
            .close_workspace_store("ws-reopen")
            .await
            .expect("close store");

        hooks.fail_open.store(true, Ordering::SeqCst);
        fixture
            .db
            .create_workspace_store("ws-reopen")
            .await
            .expect_err("injected reopen failure");
        let error = fixture
            .db
            .workspace_pool("ws-reopen")
            .await
            .expect_err("failed reopen must not permit implicit access");
        fixture.close().await;

        assert!(
            error.to_string().contains("closed"),
            "closed tombstone was lost: {error}"
        );
    }
}
