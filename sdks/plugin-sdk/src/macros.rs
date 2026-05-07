//! 驱动导出宏

/// 导出驱动的便捷宏
///
/// 此宏会自动生成FFI导出函数，使驱动可以作为动态库加载
///
/// # 示例
///
/// ```rust,ignore
/// use tinyiothub_plugin_sdk::*;
///
/// pub struct MyDriver {
///     device: Device,
/// }
///
/// impl MyDriver {
///     pub fn new(device: Device) -> Self {
///         Self { device }
///     }
///     
///     pub fn get_driver_info() -> ComponentInfo {
///         ComponentInfo {
///             name: "MyDriver".to_string(),
///             version: "1.0.0".to_string(),
///             class_name: "MyDriver".to_string(),
///             device_num: 0,
///             description: Some("My custom driver".to_string()),
///             options_descriptors: vec![],
///             location: None,
///         }
///     }
/// }
///
/// impl DeviceDriver for MyDriver {
///     // 实现trait方法...
/// #   fn device(&self) -> &Device { &self.device }
/// #   fn device_mut(&mut self) -> &mut Device { &mut self.device }
/// #   fn read_data(&mut self) -> Result<Vec<ResultValue>> { Ok(vec![]) }
/// #   fn execute_command(&mut self, _cmd: &DeviceCommand) -> Result<bool> { Ok(true) }
/// }
///
/// // 导出驱动
/// export_driver!(MyDriver);
/// ```
#[macro_export]
macro_rules! export_driver {
    ($driver_type:ty) => {
        use std::ffi::{CStr, CString};
        use std::os::raw::{c_char, c_void};

        /// 获取驱动信息
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_info() -> *const c_char {
            let info = <$driver_type>::get_driver_info();
            let json = serde_json::to_string(&info).unwrap();
            $crate::ffi::to_c_string(&json)
        }

        /// 创建驱动实例
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_create(
            device_json: *const c_char,
            _context_json: *const c_char,
        ) -> *mut c_void {
            unsafe {
                let device_str = $crate::ffi::from_c_string(device_json);
                let device: $crate::Device = serde_json::from_str(&device_str).unwrap();

                let driver = Box::new(<$driver_type>::new(device));
                Box::into_raw(driver) as *mut c_void
            }
        }

        /// 销毁驱动实例
        #[no_mangle]
        pub extern "C" fn iot_edge_driver_destroy(driver: *mut c_void) {
            unsafe {
                let _ = Box::from_raw(driver as *mut $driver_type);
            }
        }
    };
}
