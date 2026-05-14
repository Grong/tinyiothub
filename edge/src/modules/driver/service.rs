use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};
use crate::shared::error::{EdgeError, EdgeResult};

pub struct DriverService {
    #[allow(dead_code)]
    db: Arc<tinyiothub_storage::sqlite::Database>,
    scanning: AtomicBool,
    scan_timeout_secs: u64,
}

impl DriverService {
    pub fn new(
        db: Arc<tinyiothub_storage::sqlite::Database>,
        scan_timeout_secs: u64,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            scanning: AtomicBool::new(false),
            scan_timeout_secs,
        })
    }

    /// Scan all loaded drivers for devices. Returns list of discovered device IDs.
    /// Uses AtomicBool CAS to prevent concurrent scans (dedup).
    pub async fn scan_all(
        &self,
    ) -> EdgeResult<Vec<String>> {
        // Dedup: only one scan at a time
        if self.scanning.swap(true, Ordering::AcqRel) {
            return Err(EdgeError::ScanBusy);
        }

        // Yield so concurrent callers see the flag before this scan completes
        tokio::task::yield_now().await;

        // Ensure we clear the flag on return (including panic)
        let _guard = ScanGuard {
            flag: &self.scanning,
        };

        let drivers = self.list_drivers().await?;
        let mut discovered = Vec::new();

        for driver_name in drivers {
            match self.scan_single(&driver_name).await {
                Ok(devices) => discovered.extend(devices),
                Err(e) => {
                    tracing::warn!(
                        driver = %driver_name,
                        ?e,
                        "Scan failed for driver, marking unhealthy"
                    );
                }
            }
        }

        Ok(discovered)
    }

    /// Scan a single driver with timeout
    pub async fn scan_single(
        &self,
        driver_name: &str,
    ) -> EdgeResult<Vec<String>> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(self.scan_timeout_secs),
            self.do_scan(driver_name),
        )
        .await;

        match result {
            Ok(inner) => inner,
            Err(_) => Err(format!(
                "scan timeout after {}s for driver {}",
                self.scan_timeout_secs, driver_name
            )
            .into()),
        }
    }

    async fn do_scan(
        &self,
        _driver_name: &str,
    ) -> EdgeResult<Vec<String>> {
        // Delegate to tinyiothub-runtime driver registry in production
        // For now, return empty — drivers are loaded dynamically
        Ok(Vec::new())
    }

    /// Load a dynamic driver .so file with SHA256 verification
    pub async fn load_dynamic_driver(
        &self,
        name: &str,
        data: &[u8],
        expected_sha256: &str,
    ) -> EdgeResult<()> {
        // SHA256 verification
        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash = format!("{:x}", hasher.finalize());

        let expected = expected_sha256.trim_start_matches("sha256:");
        if hash != expected {
            return Err(format!(
                "SHA256 mismatch for driver {}: expected {}, got {}",
                name, expected, hash
            )
            .into());
        }

        // Size check: 10MB limit
        if data.len() > 10 * 1024 * 1024 {
            return Err(format!(
                "driver .so exceeds 10MB limit ({} bytes)",
                data.len()
            )
            .into());
        }

        // Write to temp file
        let tmp_path = std::env::temp_dir().join(format!("{}.so", name));
        std::fs::write(&tmp_path, data)?;

        // Load via libloading (unsafe — .so from cloud must be trusted at this point)
        unsafe {
            let lib = libloading::Library::new(&tmp_path)
                .map_err(|e| format!("failed to load .so: {}", e))?;

            if let Ok(init) =
                lib.get::<unsafe extern "C" fn() -> *mut std::ffi::c_void>(b"init")
            {
                let _driver_ptr = init();
            }
            // Library is intentionally leaked so the driver stays loaded
            std::mem::forget(lib);
        }

        tracing::info!(
            driver = %name,
            hash = %hash,
            "Dynamic driver loaded successfully"
        );
        Ok(())
    }

    pub async fn list_drivers(
        &self,
    ) -> EdgeResult<Vec<String>> {
        // In production: query tinyiothub-runtime driver registry
        // For now: return built-in protocols
        Ok(vec![
            "modbus".into(),
            "onvif".into(),
            "snmp".into(),
            "mqtt".into(),
        ])
    }
}

/// RAII guard that clears the scanning flag on drop
struct ScanGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> Drop for ScanGuard<'a> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
}
