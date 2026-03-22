//! 动态驱动加载器

use libloading::{Library, Symbol};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::shared::error::Error;

/// FFI函数类型定义
type GetDriverInfoFn = unsafe extern "C" fn() -> *const c_char;
type CreateDriverFn = unsafe extern "C" fn(*const c_char, *const c_char) -> *mut std::ffi::c_void;
type DestroyDriverFn = unsafe extern "C" fn(*mut std::ffi::c_void);

/// 动态驱动加载器
pub struct DynamicDriverLoader {
    library: Arc<Library>,
    driver_name: String,
    path: std::path::PathBuf,
}

impl DynamicDriverLoader {
    /// 从动态库文件加载驱动
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, Error> {
        let path = path.as_ref();
        info!("Loading dynamic driver from: {:?}", path);

        // 加载动态库
        let library = unsafe {
            Library::new(path).map_err(|e| {
                error!("Failed to load library {:?}: {}", path, e);
                Error::Unsupported(format!("Failed to load driver library: {}", e))
            })?
        };

        // 获取驱动信息
        let get_info: Symbol<GetDriverInfoFn> = unsafe {
            library.get(b"iot_edge_driver_info\0").map_err(|e| {
                error!("Failed to find iot_edge_driver_info symbol: {}", e);
                Error::Unsupported(format!(
                    "Invalid driver library: missing iot_edge_driver_info"
                ))
            })?
        };

        let info_json_ptr = unsafe { get_info() };
        let info_json = unsafe {
            CStr::from_ptr(info_json_ptr)
                .to_str()
                .map_err(|e| Error::Unsupported(format!("Invalid UTF-8 in driver info: {}", e)))?
        };

        let info: serde_json::Value = serde_json::from_str(info_json)
            .map_err(|e| Error::Unsupported(format!("Invalid driver info JSON: {}", e)))?;

        let driver_name = info["name"]
            .as_str()
            .ok_or_else(|| Error::Unsupported("Driver info missing 'name' field".to_string()))?
            .to_string();

        info!("Successfully loaded driver: {}", driver_name);
        debug!("Driver info: {}", info_json);

        Ok(Self {
            library: Arc::new(library),
            driver_name,
            path: path.to_path_buf(),
        })
    }

    /// 获取驱动名称
    pub fn driver_name(&self) -> &str {
        &self.driver_name
    }

    /// 获取驱动路径
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 获取驱动信息（JSON格式）
    pub fn get_driver_info_json(&self) -> Result<String, Error> {
        let get_info: Symbol<GetDriverInfoFn> = unsafe {
            self.library
                .get(b"iot_edge_driver_info\0")
                .map_err(|e| Error::Unsupported(format!("Failed to get symbol: {}", e)))?
        };

        let info_json_ptr = unsafe { get_info() };
        let info_json = unsafe {
            CStr::from_ptr(info_json_ptr)
                .to_str()
                .map_err(|e| Error::Unsupported(format!("Invalid UTF-8: {}", e)))?
        };

        Ok(info_json.to_string())
    }

    /// 创建驱动实例
    pub fn create_driver(&self, device_json: &str) -> Result<*mut std::ffi::c_void, Error> {
        let create_fn: Symbol<CreateDriverFn> = unsafe {
            self.library.get(b"iot_edge_driver_create\0").map_err(|e| {
                Error::Unsupported(format!("Failed to get iot_edge_driver_create: {}", e))
            })?
        };

        let device_cstr = CString::new(device_json)
            .map_err(|e| Error::Unsupported(format!("Invalid device JSON: {}", e)))?;

        // 传递空的context JSON
        let context_cstr = CString::new("{}")
            .map_err(|e| Error::Unsupported(format!("Invalid context JSON: {}", e)))?;

        let driver_ptr = unsafe { create_fn(device_cstr.as_ptr(), context_cstr.as_ptr()) };

        if driver_ptr.is_null() {
            return Err(Error::Unsupported(
                "Failed to create driver instance".to_string(),
            ));
        }

        Ok(driver_ptr)
    }

    /// 销毁驱动实例
    pub fn destroy_driver(&self, driver_ptr: *mut std::ffi::c_void) {
        if driver_ptr.is_null() {
            return;
        }

        let destroy_fn: Result<Symbol<DestroyDriverFn>, _> =
            unsafe { self.library.get(b"iot_edge_driver_destroy\0") };

        if let Ok(destroy_fn) = destroy_fn {
            unsafe { destroy_fn(driver_ptr) };
        } else {
            error!("Failed to get iot_edge_driver_destroy symbol");
        }
    }

    /// 获取库的引用计数
    pub fn library(&self) -> Arc<Library> {
        Arc::clone(&self.library)
    }
}

impl Drop for DynamicDriverLoader {
    fn drop(&mut self) {
        debug!("Unloading dynamic driver: {}", self.driver_name);
    }
}
