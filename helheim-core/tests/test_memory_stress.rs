use helheim_core::orchestra::memory::{MemoryManager, HelheimType};
use std::sync::Arc;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_memory_concurrent_snapshots_stress() {
    let base_memory = Arc::new(MemoryManager::new());
    base_memory.set_var_native("master_key".to_string(), HelheimType::Int(100));

    // Maak een basis snapshot om uit te delen
    let base_snapshot = base_memory.take_snapshot();

    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    for i in 0..100 {
        let snap_clone = base_snapshot.clone();
        
        tasks.push(tokio::spawn(async move {
            // Elke concurrent continuation spawnt een isolated memory van de snapshot
            let isolated_mem = MemoryManager::spawn_isolated(&snap_clone);
            
            // Verifieer basis state
            if let Some(HelheimType::Int(val)) = isolated_mem.get_var_native("master_key") {
                assert_eq!(val, 100);
            } else {
                panic!("master_key mist in isolated mem {}", i);
            }

            // Lokale mutatie die nergens anders mag lekken
            isolated_mem.set_var_native(format!("local_key_{}", i), HelheimType::Int(i as i64));
            
            // Simuleer een REPL sessie (snapshot -> mutate -> rollback)
            isolated_mem.snapshot();
            isolated_mem.set_var_native("temp_var".to_string(), HelheimType::String("tijdelijk".to_string()));
            assert!(isolated_mem.get_var_native("temp_var").is_some());
            
            // Nu rollbacken
            assert!(isolated_mem.rollback(1));
            assert!(isolated_mem.get_var_native("temp_var").is_none(), "Rollback faalde in isolatie");

            // Neem de uiteindelijke snapshot (zoals bij capture_continuation)
            let final_snap = isolated_mem.take_snapshot();
            assert!(final_snap.globals.contains_key(&format!("local_key_{}", i)));
            assert!(!final_snap.globals.contains_key("temp_var"));
        }));
    }

    for task in tasks {
        task.await.expect("Concurrent taak faalde");
    }

    // Verifieer dat base memory ongeschonden is
    let globals_len = base_memory.globals.len();
    assert_eq!(globals_len, 1, "Base memory is bezoedeld door concurrent executies");
}
