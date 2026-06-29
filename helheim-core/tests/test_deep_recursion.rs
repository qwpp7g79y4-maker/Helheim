use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use helheim_core::common::context::ExecutionContext;
use std::sync::Arc;

#[tokio::test]
async fn test_deep_recursion_default_stack() {
    println!("Step 1: Building string");
    let mut script = String::new();
    for _ in 0..5000 {
        script.push_str("als waar {\n");
    }
    script.push_str("    zet resultaat = \"success\";\n");
    for _ in 0..5000 {
        script.push_str("}\n");
    }

    println!("Step 2: Parsing");
    let ast = HelParser::parse(&script).expect("Parse error");
    println!("Step 3: Orchestrator created");
    let orchestrator = Arc::new(Orchestrator::new(Arc::new(DiscoveryService::new())));
    let ctx = ExecutionContext::default_privileged();

    println!("Step 4: Executing");
    let result = orchestrator.execute_ast(ast, ctx).await;
    println!("Step 5: Done Executing");
    assert!(result.is_ok(), "Deep recursion failed: {:?}", result);

    let val = orchestrator.get_var("resultaat").unwrap_or_default();
    assert_eq!(val, "success", "Variabele niet bereikt na 5000 nesting levels");
    println!("Step 6: Exiting test");
}
