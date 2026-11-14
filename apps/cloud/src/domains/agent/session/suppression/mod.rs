//! Generic source/revision/work-item suppression contract.
//!
//! The Harness only understands versions, canonical bytes and domain-separated
//! hashes here. Business semantics stay in Source adapters.

pub mod canonical;
pub mod contract;
pub mod error;
