use wasmtime::*;
pub struct TestMod { store: std::sync::Mutex<Store<()>> }
unsafe impl Send for TestMod {}
unsafe impl Sync for TestMod {}
