mod api;

pub use api::*;

#[cfg(target_arch = "wasm32")]
mod client;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn run_app() {
    client::run();
}
