/// Universele types en stubs voor Helheim
/// Voorbereid op WASM, Embedded (no_std) en ML stubs.
pub mod axioms;
pub mod context;
pub mod probe;
pub mod rune;
pub mod telemetry;

#[allow(dead_code)]
pub trait HelheimComponent {
    fn name(&self) -> &str;
    fn init(&mut self) -> Result<(), String>;
}

/// WASM Compatibility Stub
/// This module provides fallbacks or bindings for the web target.
/// Currently stubbed. Real implementation will require `web_sys` and `wasm-bindgen`.
#[cfg(target_arch = "wasm32")]
#[doc = "Phase 2 - niet in scope voor Helheim Phase 1"]
pub mod wasm_compat {
    pub fn log(s: &str) {
        // [PARKED] Phase 2: Implement web_sys::console::log_1 binding here.
        println!("[WASM LOG]: {}", s);
    }
}

/// Embedded / `no_std` Compatibility Stub
/// This module is reserved for bare-metal runtime logic, e.g. for ESP32/RPi Pico.
/// To fully support `no_std`, core components like `MemoryManager` need `alloc` fallback.
#[doc = "Phase 2 - niet in scope voor Helheim Phase 1"]
pub mod embedded {
    #[allow(dead_code)]
    pub fn init_hw() {
        // [PARKED] Phase 2: Setup GPIO, HAL, and basic panic handlers for bare-metal targets.
    }
}

/// ML Inference Stub placeholder
/// Reserved for natively loading ONNX or Burn models directly into the Helheim Runtime
/// to allow actors to call `perform ML.infer("model_name", tensor_data)`.
#[doc = "Phase 2 - niet in scope voor Helheim Phase 1"]
pub mod ml {
    #[allow(dead_code)]
    pub fn load_model(_path: &str) {
        // [PARKED] Phase 2: Integrate rust-onnxruntime or burn here.
    }
}
