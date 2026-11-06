use chrono::{DateTime, Utc};
use tinyiothub_core::media::{MediaError, MediaEvidenceRecord};

use crate::Db;

#[derive(sqlx::FromRow)]
struct EvidenceRow {
    id: String,
    workspace_id: String,
    task_id: Option<String>,
    run_id: Option<String>,
    tool_call_id: Option<String>,
    job_id: Option<String>,
    device_id: String,
    source_id: String,
    source_seq: i64,
    media_time: DateTime<Utc>,
    mime: String,
    size_bytes: i64,
    sha256: String,
    storage_path: String,
    dedup_key: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

impl TryFrom<EvidenceRow> for MediaEvidenceRecord {
    type Error = MediaError;
    fn try_from(row: EvidenceRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            workspace_id: row.workspace_id,
            task_id: row.task_id,
            run_id: row.run_id,
            tool_call_id: row.tool_call_id,
            job_id: row.job_id,
            device_id: row.device_id,
            source_id: row.source_id,
            source_seq: u64::try_from(row.source_seq)
                .map_err(|_| MediaError::Storage("negative source sequence".into()))?,
            media_time: row.media_time,
            mime: row.mime,
            size_bytes: u64::try_from(row.size_bytes).map_err(|_| MediaError::Storage("negative media size".into()))?,
            sha256: row.sha256,
            storage_path: row.storage_path,
            dedup_key: row.dedup_key,
            created_at: row.created_at,
            expires_at: row.expires_at,
        })
    }
}

fn storage(error: impl std::fmt::Display) -> MediaError {
    MediaError::Storage(error.to_string())
}

impl Db {
    pub async fn put_media_evidence(&self, row: &MediaEvidenceRecord) -> Result<MediaEvidenceRecord, MediaError> {
        self.put_media_evidence_and_event(row).await.map(|(stored, _)| stored)
    }

    pub async fn put_media_evidence_and_event(
        &self,
        row: &MediaEvidenceRecord,
    ) -> Result<(MediaEvidenceRecord, Option<crate::runtime_event::RuntimeEvent>), MediaError> {
        let terminal = match row.task_id.as_deref() {
            Some(task_id) => {
                let has_store = self
                    .try_existing_workspace_pool(&row.workspace_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some();
                has_store
                    && matches!(
                        self.harness_get_task(&row.workspace_id, task_id).await,
                        Ok(Some(task)) if task.status.is_terminal()
                    )
            }
            None => false,
        };
        let mut tx = self.pool().begin().await.map_err(storage)?;
        sqlx::query(
            "INSERT INTO media_evidence (id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id, source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key, created_at, expires_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(dedup_key) DO NOTHING",
        )
        .bind(&row.id).bind(&row.workspace_id).bind(&row.task_id).bind(&row.run_id)
        .bind(&row.tool_call_id).bind(&row.job_id).bind(&row.device_id).bind(&row.source_id)
        .bind(i64::try_from(row.source_seq).map_err(|_| MediaError::Validation("source sequence is too large".into()))?)
        .bind(row.media_time).bind(&row.mime)
        .bind(i64::try_from(row.size_bytes).map_err(|_| MediaError::Validation("media size is too large".into()))?)
        .bind(&row.sha256).bind(&row.storage_path).bind(&row.dedup_key).bind(row.created_at).bind(row.expires_at)
        .execute(&mut *tx).await.map_err(storage)?;
        let stored = sqlx::query_as::<_, EvidenceRow>("SELECT id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id, source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key, created_at, expires_at FROM media_evidence WHERE dedup_key = ?")
            .bind(&row.dedup_key).fetch_one(&mut *tx).await.map_err(storage)?;
        let stored = MediaEvidenceRecord::try_from(stored)?;
        let event = if stored.task_id.is_some() {
            Some(
                crate::runtime_event::append_runtime_event_tx_with_policy(
                    &mut tx,
                    crate::runtime_event::RuntimeEventInput {
                        event_id: format!("evidence:{}:created", stored.id),
                        event_type: "evidence.created".into(),
                        occurred_at: stored.created_at,
                        workspace_id: stored.workspace_id.clone(),
                        task_id: stored.task_id.clone(),
                        run_id: stored.run_id.clone(),
                        tool_call_id: stored.tool_call_id.clone(),
                        job_id: stored.job_id.clone(),
                        approval_id: None,
                        device_id: Some(stored.device_id.clone()),
                        source_id: Some(stored.source_id.clone()),
                        source_seq: Some(
                            i64::try_from(stored.source_seq)
                                .map_err(|_| MediaError::Validation("source sequence is too large".into()))?,
                        ),
                        evidence_id: Some(stored.id.clone()),
                        data: serde_json::json!({
                            "mime": stored.mime,
                            "mediaTime": stored.media_time,
                            "expiresAt": stored.expires_at,
                            "sourceSeq": stored.source_seq,
                        }),
                    },
                    !terminal,
                )
                .await
                .map_err(storage)?,
            )
        } else {
            None
        };
        tx.commit().await.map_err(storage)?;
        Ok((stored, event))
    }

    pub async fn get_media_evidence(&self, evidence_id: &str) -> Result<Option<MediaEvidenceRecord>, MediaError> {
        sqlx::query_as::<_, EvidenceRow>("SELECT id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id, source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key, created_at, expires_at FROM media_evidence WHERE id = ?")
            .bind(evidence_id).fetch_optional(self.pool()).await.map_err(storage)?
            .map(MediaEvidenceRecord::try_from).transpose()
    }

    pub async fn get_media_evidence_in_workspace(
        &self,
        workspace_id: &str,
        evidence_id: &str,
    ) -> Result<Option<MediaEvidenceRecord>, MediaError> {
        sqlx::query_as::<_, EvidenceRow>("SELECT id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id, source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key, created_at, expires_at FROM media_evidence WHERE workspace_id = ? AND id = ?")
            .bind(workspace_id).bind(evidence_id).fetch_optional(self.pool()).await.map_err(storage)?
            .map(MediaEvidenceRecord::try_from).transpose()
    }

    pub async fn delete_expired_media(&self, now: DateTime<Utc>) -> Result<Vec<MediaEvidenceRecord>, MediaError> {
        // DELETE ... RETURNING is write-first, so this path never upgrades a
        // deferred SHARED lock into RESERVED while another writer is active.
        let rows = sqlx::query_as::<_, EvidenceRow>("DELETE FROM media_evidence WHERE expires_at <= ? RETURNING id, workspace_id, task_id, run_id, tool_call_id, job_id, device_id, source_id, source_seq, media_time, mime, size_bytes, sha256, storage_path, dedup_key, created_at, expires_at")
            .bind(now)
            .fetch_all(self.pool())
            .await
            .map_err(storage)?;
        rows.into_iter().map(MediaEvidenceRecord::try_from).collect()
    }

    pub async fn media_storage_is_referenced(&self, path: &str) -> Result<bool, MediaError> {
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM media_evidence WHERE storage_path = ?)")
            .bind(path)
            .fetch_one(self.pool())
            .await
            .map_err(storage)
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use tinyiothub_core::media::MediaEvidenceRecord;

    use crate::Db;

    fn row() -> MediaEvidenceRecord {
        let now = Utc::now();
        MediaEvidenceRecord {
            id: "evidence-1".into(),
            workspace_id: "workspace-1".into(),
            task_id: None,
            run_id: Some("run-1".into()),
            tool_call_id: Some("call-1".into()),
            job_id: None,
            device_id: "device-1".into(),
            source_id: "front".into(),
            source_seq: 7,
            media_time: now,
            mime: "image/png".into(),
            size_bytes: 12,
            sha256: "abc".into(),
            storage_path: "7773/abc".into(),
            dedup_key: "dedup-1".into(),
            created_at: now,
            expires_at: now + Duration::days(7),
        }
    }

    #[tokio::test]
    async fn media_evidence_dedupes_and_expires_only_old_rows() {
        let db = Db::new(crate::test_helpers::test_pool().await);
        sqlx::query(
            "INSERT OR IGNORE INTO subscription_plans (id, name, display_name) VALUES ('plan_free','free','Free')",
        )
        .execute(db.pool())
        .await
        .expect("plan");
        sqlx::query("INSERT OR IGNORE INTO tenants (id, name, slug) VALUES ('t1','tenant','t1')")
            .execute(db.pool())
            .await
            .expect("tenant");
        sqlx::query("INSERT INTO workspaces (id, name, tenant_id, created_at, updated_at) VALUES (?, 'workspace', 't1', 'now', 'now')")
            .bind("workspace-1").execute(db.pool()).await.expect("workspace");
        let original = row();
        let first = db.put_media_evidence(&original).await.expect("insert");
        assert_eq!(first.id, original.id);
        assert!(
            db.get_media_evidence_in_workspace("workspace-1", "evidence-1")
                .await
                .expect("scoped lookup")
                .is_some()
        );
        assert!(
            db.get_media_evidence_in_workspace("workspace-2", "evidence-1")
                .await
                .expect("foreign lookup")
                .is_none(),
            "evidence metadata must not leak across workspaces"
        );
        let mut retry = original.clone();
        retry.id = "evidence-retry".into();
        assert_eq!(db.put_media_evidence(&retry).await.expect("dedupe").id, original.id);
        assert!(
            db.media_storage_is_referenced(&original.storage_path)
                .await
                .expect("reference")
        );
        assert!(
            db.delete_expired_media(Utc::now())
                .await
                .expect("not expired")
                .is_empty()
        );
        assert_eq!(
            db.delete_expired_media(Utc::now() + Duration::days(8))
                .await
                .expect("expired")
                .len(),
            1
        );
        assert!(
            !db.media_storage_is_referenced(&original.storage_path)
                .await
                .expect("deleted")
        );
    }
}
