//! Rule engine — evaluates conditions and triggers actions.
//!
//! TODO: Migrate from `cloud/src/domain/alarm/services/rule_engine.rs`.

pub mod action;
pub mod evaluator;
pub mod parser;

pub use action::RuleAction;
pub use evaluator::RuleEvaluator;
pub use parser::RuleParser;
