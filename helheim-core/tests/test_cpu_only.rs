use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use std::sync::Arc;

async fn run_helheim_script(script: &str) -> Arc<Orchestrator> {
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    let ast = HelParser::parse(script).expect("Parse error in test script!");
    let mut linker = helheim_core::orchestra::resolver::ModuleLinker::with_std_lib(
        std::path::PathBuf::from("."),
        std::path::PathBuf::from(".")
    );
    let linked_ast = linker.link(ast, std::path::Path::new("test_script.hel")).expect("Linker error in test script!");
    let ctx = helheim_core::common::context::ExecutionContext::default_privileged();
    let _ = orchestrator.execute_ast(linked_ast, ctx).await;
    orchestrator
}

#[tokio::test]
async fn test_cpu_fallback_execution() {
    // We force the CPU fallback mode
    unsafe { std::env::set_var("HELHEIM_DISABLE_CUDA", "1"); }

    let script = r#"
        zet base = 10;
        zet acc = 0;
        zet i = 1;
        zolang i < 6 {
            zet acc = acc + i;
            zet i = i + 1;
        }
        zet res = base + acc;
    "#;

    let engine = run_helheim_script(script).await;
    let res: i32 = engine.get_var("res").unwrap_or_default().parse().unwrap_or(-1);
    
    // Sum of 1..5 is 15. Base is 10.
    assert_eq!(res, 25, "CPU Fallback computation failed or returned wrong result");

    // Clean up
    unsafe { std::env::remove_var("HELHEIM_DISABLE_CUDA"); }
}
