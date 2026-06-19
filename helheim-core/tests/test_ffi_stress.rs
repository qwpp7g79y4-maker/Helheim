use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinSet;

use helheim_core::orchestra::stdlib_manager::StdLibManager;
use helheim_core::ffi::{HelValue, HEL_ERR_OK, unmarshal_helvalue_to_helheimtype};
use helheim_lang::memory::HelheimType;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_ffi_hot_reload() {
    // [W·AG·AF] C1 Review: Concurrent FFI + hot-reload stress test.
    let stdlib = Arc::new(StdLibManager::new());
    
    // 1. Initial Load of libsqlite.so
    {
        let mut loader = stdlib.native_modules.lock().await;
        loader.add_search_path(std::path::PathBuf::from("../test_plugins"));
        // Assume test_plugins/libsqlite.so is present (built via stdlib_ffi_completed.md)
        let loaded = loader.load("sqlite", std::ptr::null_mut());
        assert!(loaded.is_ok(), "Failed to load libsqlite.so");
    }

    let mut set = JoinSet::new();

    // 2. Spawn 20 worker tasks that constantly call sqlite::version
    for worker_id in 0..20 {
        let stdlib_clone = stdlib.clone();
        set.spawn(async move {
            let mut success_count = 0;
            for _ in 0..50 {
                // Yield to ensure interleaving
                tokio::task::yield_now().await;
                
                let loader = stdlib_clone.native_modules.lock().await;
                let module = loader.get("sqlite");
                if let Some(loaded_module) = module {
                    // Extract function pointer
                    if let Some(&func) = loaded_module.functions.get("sqlite::open") {
                        // Drop lock BEFORE calling FFI to allow concurrent reloading!
                        // The Arc<LoadedNativeModule> keeps the library alive in memory!
                        let module_arc = loaded_module.clone();
                        drop(loader);
                        
                        let mut ctx = module_arc.context.lock().unwrap();
                        let args = vec![HelValue::string_borrowed(":memory:")];
                        let mut out = HelValue::NULL;
                        
                        // Call native function
                        let res = func(&mut *ctx as *mut _, args.as_ptr(), 1, &mut out);
                        
                        assert_eq!(res, HEL_ERR_OK);
                        let ht = unsafe { unmarshal_helvalue_to_helheimtype(out, &mut *ctx as *mut _) };
                        if let HelheimType::ResourceHandle { kind, id: _ } = ht {
                            assert_eq!(kind, "sqlite");
                            success_count += 1;
                        }
                    } else {
                        drop(loader);
                    }
                } else {
                    drop(loader);
                }
            }
            assert_eq!(success_count, 50, "Worker {} did not complete all FFI calls", worker_id);
        });
    }

    // 3. Spawn a reloader task that continuously hot-reloads the module
    let stdlib_reloader = stdlib.clone();
    set.spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let mut loader = stdlib_reloader.native_modules.lock().await;
            // Hot reload while other threads hold Arc<LoadedNativeModule> and execute functions!
            let reload_res = loader.reload("sqlite", std::ptr::null_mut());
            assert!(reload_res.is_ok(), "Hot reload failed");
        }
    });

    // Wait for all tasks to finish
    while let Some(res) = set.join_next().await {
        res.expect("Task panicked");
    }
}
