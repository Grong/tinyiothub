//! Driver module — re-exported from tinyiothub-engine
//!
//! The canonical implementation lives in `tinyiothub-engine` so it can be
//! shared by cloud, edge, and any other binaries.

pub use tinyiothub_engine::driver::*;

// Re-export dynamic sub-module so existing callers keep working.
#[doc(inline)]
pub use tinyiothub_engine::driver::dynamic;
