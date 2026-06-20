use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use std::sync::Arc;
use std::path::PathBuf;

#[tokio::test]
async fn test_wasm_ffi_sandboxing_math() {
    let _ = tracing_subscriber::fmt::try_init();
    
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    
    // Configure StdLibManager to search in our actual library directory
    {
        let mut loader = orchestrator.executor.stdlib.native_modules.lock().await;
        // The project root during tests is `helheim-core`. 
        // Our math.wasm is in `../stdlib/lib/math.wasm`
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("..");
        path.push("stdlib");
        path.push("lib");
        loader.add_search_path(path);
    }

    let script = r#"
        gebruik "math";
        
        // This invokes our math_sin function compiled to WebAssembly!
        zet s = roep_aan math::sin 0.0;
        
        // This invokes math_add
        zet a = roep_aan math::add 5 7;
    "#;

    let ast = HelParser::parse(script).expect("Parse error in test script!");
    let mut linker = helheim_core::orchestra::resolver::ModuleLinker::with_std_lib(
        std::path::PathBuf::from("."),
        std::path::PathBuf::from(".")
    );
    let linked_ast = linker.link(ast, std::path::Path::new("test_script.hel")).expect("Linker error in test script!");
    let ctx = helheim_core::common::context::ExecutionContext::default_privileged();
    
    let result = orchestrator.execute_ast(linked_ast, ctx).await;
    assert!(result.is_ok(), "Execution failed: {:?}", result);
    
    let s_val = orchestrator.get_var("s").unwrap();
    let a_val = orchestrator.get_var("a").unwrap();
    
    assert_eq!(s_val, "0"); // sin(0) = 0
    assert_eq!(a_val, "12"); // 5 + 7 = 12
}
