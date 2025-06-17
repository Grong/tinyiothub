use loco_rs::prelude::Routes;

pub mod console;
pub mod iot;

pub fn routes() -> Vec<Routes> {
    vec![console::routes(), iot::routes()]
        .into_iter()
        .flatten()
        .collect()
}