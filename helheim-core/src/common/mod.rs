/// Universele types en stubs voor Helheim
/// Voorbereid op WASM, Embedded (no_std) en ML stubs.
pub mod axioms;
pub mod context;
pub mod parser;
pub mod probe;
pub mod rune;
pub mod telemetry;

#[allow(dead_code)]
pub trait HelheimComponent {
    fn name(&self) -> &str;
    fn init(&mut self) -> Result<(), String>;
}

// WASM Stub
#[cfg(target_arch = "wasm32")]
pub mod wasm_compat {
    pub fn log(s: &str) {
        // In een echte WASM build zouden we hier web_sys gebruiken
        println!("[WASM LOG]: {}", s);
    }
}

// Embedded / No_std Stub placeholder
pub mod embedded {
    #[allow(dead_code)]
    pub fn init_hw() {
        // GPIO / HAL initialisatie voor RPi/Arduino
    }
}

// ML Inference Stub placeholder
pub mod ml {
    #[allow(dead_code)]
    pub fn load_model(_path: &str) {
        // Burn of ONNXRuntime integratie
    }
}
