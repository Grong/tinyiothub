//! 引擎适配器：port 接口的具体引擎实现。
//!
//! - `zeroclaw`：phase 1 引擎（zeroclaw 退化为黑盒实现细节）
//! - `rig`：phase 2 引擎（Task 7 加入）
pub mod rig;
pub mod zeroclaw;
