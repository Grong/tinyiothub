//! Notification channel plugin stub
use std::sync::Arc;
use crate::shared::error::Error;

pub fn create_handler(_config: &toml::Value, _context: Arc<super::AppContext>) -> Result<Box<dyn super::PluginHandler>, Error> {
    Err(Error::Unsupported("notification plugin not implemented yet".into()))
}
