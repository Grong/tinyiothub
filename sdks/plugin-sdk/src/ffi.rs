//! FFI辅助函数

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// 将Rust字符串转换为C字符串指针
/// 
/// # Safety
/// 
/// 调用者负责使用 `free_c_string` 释放返回的指针
pub fn to_c_string(s: &str) -> *const c_char {
    CString::new(s).unwrap().into_raw()
}

/// 从C字符串指针读取Rust字符串
/// 
/// # Safety
/// 
/// `ptr` 必须是有效的C字符串指针
pub unsafe fn from_c_string(ptr: *const c_char) -> String {
    CStr::from_ptr(ptr).to_string_lossy().into_owned()
}

/// 释放C字符串
/// 
/// # Safety
/// 
/// `ptr` 必须是通过 `to_c_string` 创建的指针
pub unsafe fn free_c_string(ptr: *const c_char) {
    if !ptr.is_null() {
        let _ = CString::from_raw(ptr as *mut c_char);
    }
}
