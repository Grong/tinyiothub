//! Legacy device management-plane code, extracted from
//! `cloud::modules::device` (P4 pilot). These services predate the thing
//! domain model and are kept here until the driver crate (Task 20) and the
//! alarm/monitoring extractions (Tasks 18/19) unblock the remaining pieces
//! (diagnostics, monitoring, performance, query services still live in
//! cloud).

pub mod device_query;
pub mod trace;
pub mod trace_repository;
pub mod types;
