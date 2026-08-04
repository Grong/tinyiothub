//! Rule engine — evaluates conditions and triggers actions.

pub mod evaluator;
pub mod parser;

pub use evaluator::RuleEvaluator;
pub use parser::RuleParser;
