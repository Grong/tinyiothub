use loco_rs::prelude::Routes;

pub mod console;

pub fn routes() -> Vec<Routes> {
    console::routes()
}