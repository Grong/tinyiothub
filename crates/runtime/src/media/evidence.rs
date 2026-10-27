use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use image::ImageReader;
use serde_json::json;
use sha2::{Digest, Sha256};
use tinyiothub_core::media::{EvidenceRef, ImageInput, MediaAttachment, MediaError, MediaEvidenceRecord};

use crate::ports::MediaEvidenceStore;

pub const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_IMAGE_PIXELS: u64 = 16_000_000;

#[async_trait]
pub trait MediaStore: Send + Sync {
    async fn put(&self, workspace_id: &str, sha256: &str, bytes: &[u8]) -> Result<String, MediaError>;
    async fn read(&self, storage_path: &str) -> Result<Vec<u8>, MediaError>;
    async fn delete(&self, storage_path: &str, grace: Duration) -> Result<(), MediaError>;
}

pub struct LocalMediaStore {
    root: PathBuf,
}

impl LocalMediaStore {
    pub async fn new(root: impl AsRef<Path>) -> Result<Self, MediaError> {
        let root = root.as_ref().to_path_buf();
        tokio::fs::create_dir_all(&root).await.map_err(storage_error)?;
        Ok(Self { root })
    }

    fn checked_path(&self, storage_path: &str) -> Result<PathBuf, MediaError> {
        let relative = Path::new(storage_path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err(MediaError::Storage("invalid internal media path".into()));
        }
        Ok(self.root.join(relative))
    }
}

fn storage_error(error: std::io::Error) -> MediaError {
    MediaError::Storage(error.to_string())
}

#[async_trait]
impl MediaStore for LocalMediaStore {
    async fn put(&self, workspace_id: &str, sha256: &str, bytes: &[u8]) -> Result<String, MediaError> {
        if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(MediaError::Validation("media object key must be a SHA-256 hash".into()));
        }
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(MediaError::ImageTooLarge);
        }
        let directory = self.root.join(hex::encode(workspace_id.as_bytes()));
        tokio::fs::create_dir_all(&directory).await.map_err(storage_error)?;
        let temporary = directory.join(format!(".tmp-{}", rand::random::<u64>()));
        tokio::fs::write(&temporary, bytes).await.map_err(storage_error)?;
        let written = tokio::fs::read(&temporary).await.map_err(storage_error)?;
        if written.len() != bytes.len() || hex::encode(Sha256::digest(&written)) != sha256 {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(MediaError::Storage("stored media verification failed".into()));
        }
        let destination = directory.join(sha256);
        if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
            let _ = tokio::fs::remove_file(&temporary).await;
            return Err(storage_error(error));
        }
        let relative = Path::new(&hex::encode(workspace_id.as_bytes())).join(sha256);
        Ok(relative.to_string_lossy().into_owned())
    }

    async fn read(&self, storage_path: &str) -> Result<Vec<u8>, MediaError> {
        tokio::fs::read(self.checked_path(storage_path)?)
            .await
            .map_err(storage_error)
    }

    async fn delete(&self, storage_path: &str, grace: Duration) -> Result<(), MediaError> {
        let path = self.checked_path(storage_path)?;
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(storage_error(error)),
        };
        let modified = metadata.modified().map_err(storage_error)?;
        let age = modified
            .elapsed()
            .map_err(|error| MediaError::Storage(error.to_string()))?;
        if age >= grace {
            match tokio::fs::remove_file(path).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(storage_error(error)),
            }
        }
        Ok(())
    }
}

pub struct EvidenceService {
    store: Arc<dyn MediaStore>,
    evidence: Arc<dyn MediaEvidenceStore>,
}

impl EvidenceService {
    pub fn new(store: Arc<dyn MediaStore>, evidence: Arc<dyn MediaEvidenceStore>) -> Self {
        Self { store, evidence }
    }

    pub async fn put_image(&self, input: ImageInput) -> Result<EvidenceRef, MediaError> {
        validate_image(&input)?;
        let sha256 = hex::encode(Sha256::digest(&input.bytes));
        let dedup_key = evidence_dedup_key(&input, &sha256)?;
        let storage_path = self.store.put(&input.workspace_id, &sha256, &input.bytes).await?;
        let now = Utc::now();
        let row = MediaEvidenceRecord {
            id: tinyiothub_core::generate_id(),
            workspace_id: input.workspace_id,
            task_id: input.task_id,
            run_id: input.run_id,
            tool_call_id: input.tool_call_id,
            job_id: input.job_id,
            device_id: input.device_id,
            source_id: input.source_id,
            source_seq: input.source_seq,
            media_time: input.media_time,
            mime: input.mime,
            size_bytes: input.bytes.len() as u64,
            sha256,
            storage_path,
            dedup_key,
            created_at: now,
            expires_at: now + chrono::Duration::days(7),
        };
        let stored = self.evidence.insert_or_get(&row).await?;
        Ok(EvidenceRef {
            evidence_id: stored.id,
            mime: stored.mime,
            source_seq: stored.source_seq,
            media_time: stored.media_time,
        })
    }

    pub async fn get_image(&self, evidence_id: &str) -> Result<Option<MediaAttachment>, MediaError> {
        let Some(row) = self.evidence.get(evidence_id).await? else {
            return Ok(None);
        };
        let bytes = self.store.read(&row.storage_path).await?;
        if bytes.len() as u64 != row.size_bytes || hex::encode(Sha256::digest(&bytes)) != row.sha256 {
            return Err(MediaError::Storage("stored media verification failed".into()));
        }
        Ok(Some(MediaAttachment {
            evidence_id: row.id,
            mime: row.mime,
            bytes,
            source_seq: row.source_seq,
            media_time: row.media_time,
        }))
    }

    pub async fn cleanup_expired(&self, now: DateTime<Utc>, grace: Duration) -> Result<(), MediaError> {
        for row in self.evidence.delete_expired(now).await? {
            if !self.evidence.storage_is_referenced(&row.storage_path).await? {
                self.store.delete(&row.storage_path, grace).await?;
            }
        }
        Ok(())
    }
}

fn validate_image(input: &ImageInput) -> Result<(), MediaError> {
    if input.bytes.len() > MAX_IMAGE_BYTES {
        return Err(MediaError::ImageTooLarge);
    }
    let expected = match input.mime.as_str() {
        "image/png" => image::ImageFormat::Png,
        "image/jpeg" => image::ImageFormat::Jpeg,
        other => return Err(MediaError::UnsupportedMime(other.to_owned())),
    };
    if image::guess_format(&input.bytes).map_err(|_| MediaError::InvalidImage)? != expected {
        return Err(MediaError::InvalidImage);
    }
    let now = Utc::now();
    let oldest = now - chrono::Duration::from_std(input.max_age).map_err(|_| MediaError::StaleMedia)?;
    if input.media_time < oldest || input.media_time > now + chrono::Duration::seconds(5) {
        return Err(MediaError::StaleMedia);
    }
    let (width, height) = ImageReader::with_format(Cursor::new(&input.bytes), expected)
        .into_dimensions()
        .map_err(|_| MediaError::InvalidImage)?;
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_IMAGE_PIXELS {
        return Err(MediaError::ImageDimensions);
    }
    Ok(())
}

pub fn evidence_dedup_key(input: &ImageInput, sha256: &str) -> Result<String, MediaError> {
    let tuple = json!([
        input.workspace_id,
        input.task_id,
        input.run_id,
        input.tool_call_id,
        input.job_id,
        input.device_id,
        input.source_id,
        input.source_seq,
        sha256
    ]);
    let canonical = serde_json::to_vec(&tuple).map_err(|error| MediaError::Validation(error.to_string()))?;
    Ok(hex::encode(Sha256::digest(&canonical)))
}

#[derive(Default)]
pub struct RunMediaBudget {
    image_bytes: usize,
}

impl RunMediaBudget {
    pub fn record(&mut self, bytes: usize) -> Result<(), MediaError> {
        let next = self.image_bytes.saturating_add(bytes);
        if next > 8 * 1024 * 1024 {
            return Err(MediaError::RunImageBudget);
        }
        self.image_bytes = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc, time::Duration};

    use async_trait::async_trait;
    use chrono::Utc;
    use tinyiothub_core::media::{ImageInput, MediaError, MediaEvidenceRecord};

    use super::{EvidenceService, LocalMediaStore, RunMediaBudget};
    use crate::ports::MediaEvidenceStore;

    #[derive(Default)]
    struct MemoryEvidence {
        rows: tokio::sync::Mutex<HashMap<String, MediaEvidenceRecord>>,
    }

    #[async_trait]
    impl MediaEvidenceStore for MemoryEvidence {
        async fn insert_or_get(&self, row: &MediaEvidenceRecord) -> Result<MediaEvidenceRecord, MediaError> {
            let mut rows = self.rows.lock().await;
            if let Some(existing) = rows.values().find(|item| item.dedup_key == row.dedup_key) {
                return Ok(existing.clone());
            }
            rows.insert(row.id.clone(), row.clone());
            Ok(row.clone())
        }
        async fn get(&self, evidence_id: &str) -> Result<Option<MediaEvidenceRecord>, MediaError> {
            Ok(self.rows.lock().await.get(evidence_id).cloned())
        }
        async fn delete_expired(&self, now: chrono::DateTime<Utc>) -> Result<Vec<MediaEvidenceRecord>, MediaError> {
            let mut rows = self.rows.lock().await;
            let expired: Vec<_> = rows.values().filter(|row| row.expires_at <= now).cloned().collect();
            for row in &expired {
                rows.remove(&row.id);
            }
            Ok(expired)
        }
        async fn storage_is_referenced(&self, path: &str) -> Result<bool, MediaError> {
            Ok(self.rows.lock().await.values().any(|row| row.storage_path == path))
        }
    }

    struct EvidenceFixture {
        service: Arc<EvidenceService>,
        evidence: Arc<MemoryEvidence>,
        root: Arc<tempfile::TempDir>,
    }

    impl EvidenceFixture {
        async fn new() -> Self {
            let root = Arc::new(tempfile::tempdir().expect("media tempdir"));
            let evidence = Arc::new(MemoryEvidence::default());
            let store = Arc::new(LocalMediaStore::new(root.path()).await.expect("media store"));
            let service = Arc::new(EvidenceService::new(store, evidence.clone()));
            Self {
                service,
                evidence,
                root,
            }
        }
        async fn evidence_row_count(&self) -> usize {
            self.evidence.rows.lock().await.len()
        }
        async fn stored_object_count(&self) -> usize {
            let mut count = 0;
            let mut workspaces = tokio::fs::read_dir(self.root.path()).await.expect("root");
            while let Some(workspace) = workspaces.next_entry().await.expect("workspace") {
                let mut objects = tokio::fs::read_dir(workspace.path()).await.expect("objects");
                while objects.next_entry().await.expect("object").is_some() {
                    count += 1;
                }
            }
            count
        }
    }

    fn png_bytes() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(1, 1, image::Rgba([1, 2, 3, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("PNG");
        bytes.into_inner()
    }

    fn image_input(mime: &str, size: usize) -> ImageInput {
        let mut bytes = png_bytes();
        bytes.resize(size.max(bytes.len()), 0);
        ImageInput {
            workspace_id: "workspace/unsafe".into(),
            task_id: Some("task-1".into()),
            run_id: Some("run-1".into()),
            tool_call_id: Some("call-1".into()),
            job_id: None,
            device_id: "device-1".into(),
            source_id: "front".into(),
            source_seq: 7,
            media_time: Utc::now(),
            mime: mime.into(),
            bytes,
            max_age: Duration::from_secs(60),
        }
    }

    #[tokio::test]
    async fn local_store_rejects_non_hash_object_paths() {
        use super::MediaStore;

        let root = tempfile::tempdir().expect("media tempdir");
        let store = LocalMediaStore::new(root.path()).await.expect("media store");
        let outside = root.path().join("outside");
        let result = store
            .put("workspace", outside.to_str().expect("UTF-8 path"), &png_bytes())
            .await;
        assert!(matches!(result, Err(MediaError::Validation(_))));
        assert!(!outside.exists());
    }

    #[tokio::test]
    async fn rejects_svg_oversize_and_run_budget() {
        let fixture = EvidenceFixture::new().await;
        assert!(matches!(
            fixture.service.put_image(image_input("image/svg+xml", 12)).await,
            Err(MediaError::UnsupportedMime(_))
        ));
        assert!(matches!(
            fixture
                .service
                .put_image(image_input("image/png", 2 * 1024 * 1024 + 1))
                .await,
            Err(MediaError::ImageTooLarge)
        ));
        let mut run = RunMediaBudget::default();
        for _ in 0..4 {
            run.record(2 * 1024 * 1024).expect("within 8 MiB");
        }
        assert!(matches!(run.record(1), Err(MediaError::RunImageBudget)));
    }

    #[tokio::test]
    async fn rejects_magic_dimensions_and_stale_media() {
        let fixture = EvidenceFixture::new().await;
        let mut bad_magic = image_input("image/png", 12);
        bad_magic.bytes = b"\x89PNG\r\n\x1a\nnot an image".to_vec();
        assert!(matches!(
            fixture.service.put_image(bad_magic).await,
            Err(MediaError::InvalidImage)
        ));
        let mut dimensions = image_input("image/png", 100);
        dimensions.bytes[16..20].copy_from_slice(&4001_u32.to_be_bytes());
        dimensions.bytes[20..24].copy_from_slice(&4000_u32.to_be_bytes());
        dimensions.bytes[29..33].copy_from_slice(&0x01bd883c_u32.to_be_bytes());
        assert!(matches!(
            fixture.service.put_image(dimensions).await,
            Err(MediaError::ImageDimensions)
        ));
        let mut stale = image_input("image/png", 100);
        stale.media_time = Utc::now() - chrono::Duration::minutes(2);
        assert!(matches!(
            fixture.service.put_image(stale).await,
            Err(MediaError::StaleMedia)
        ));
    }

    #[tokio::test]
    async fn stored_evidence_loads_exact_bytes_and_safe_metadata() {
        let fixture = EvidenceFixture::new().await;
        let input = image_input("image/png", 1024);
        let expected = input.bytes.clone();
        let reference = fixture.service.put_image(input).await.expect("stored");
        let attachment = fixture
            .service
            .get_image(&reference.evidence_id)
            .await
            .expect("read")
            .expect("present");
        assert_eq!(attachment.bytes, expected);
        assert_eq!(attachment.evidence_id, reference.evidence_id);
        fixture
            .service
            .cleanup_expired(Utc::now() + chrono::Duration::days(8), Duration::ZERO)
            .await
            .expect("cleanup");
        assert!(
            fixture
                .service
                .get_image(&reference.evidence_id)
                .await
                .expect("gone")
                .is_none()
        );
    }

    #[tokio::test]
    async fn concurrent_same_evidence_reuses_row_and_bytes() {
        let fixture = EvidenceFixture::new().await;
        let input = image_input("image/png", 1024);
        let (a, b) = tokio::join!(
            fixture.service.put_image(input.clone()),
            fixture.service.put_image(input)
        );
        assert_eq!(a.expect("first").evidence_id, b.expect("retry").evidence_id);
        assert_eq!(fixture.evidence_row_count().await, 1);
        assert_eq!(fixture.stored_object_count().await, 1);
    }
}
