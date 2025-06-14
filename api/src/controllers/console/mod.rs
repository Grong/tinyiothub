use loco_rs::prelude::*;

pub mod feature;
pub mod init;
pub mod setup;
pub mod auth;
pub mod apps;
pub mod version;
pub mod workspace;
pub mod tags;

pub fn routes() -> Vec<Routes> {
    vec![
        apps::routes(),
        auth::routes(),
        feature::routes(),
        init::routes(),
        setup::routes(),
        version::routes(),
        workspace::routes(),
        tags::routes(),
    ]
}