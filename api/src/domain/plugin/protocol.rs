//! Protocol driver plugin stub
use crate::shared::error::Error;

pub fn create_handler(_config: &toml::Value) -> Result<Box<dyn super::PluginHandler>, Error> {
    Err(Error::Unsupported("protocol plugin not implemented yet".into()))
}
