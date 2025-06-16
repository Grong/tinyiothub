pub mod device;
pub mod device_event;
pub mod device_template;

use loco_rs::prelude::Routes;

pub fn routes() -> Vec<Routes> {
    vec![
        device::routes(),
        device_event::routes(),
        device_template::routes(),
    ]
}
