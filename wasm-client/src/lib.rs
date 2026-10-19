use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn greet(name: &str) -> String {
    format!("Hello, {} from wasm-client!", name)
}

#[wasm_bindgen(start)]
pub fn start() {
    // set a simple console message to show WASM loaded
    web_sys::console::log_1(&"wasm-client loaded".into());
}
