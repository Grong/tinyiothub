use loco_rs::prelude::*;

pub mod apps;
pub mod auth;
pub mod feature;
pub mod init;
pub mod setup;
pub mod tags;
pub mod version;
pub mod workspace;

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
