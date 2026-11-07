use std::time::Duration;

pub const ASK_USER_TIMEOUT: Duration = Duration::from_secs(10 * 60);
pub const SESSION_TURN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub mod context;
pub mod events;
pub mod questions;
pub mod todos;
pub mod tools;
pub mod types;

pub use context::*;
pub use events::*;
pub use questions::QuestionBroker;
pub use todos::TodoStore;
pub use types::*;
