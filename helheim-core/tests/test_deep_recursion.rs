use helheim_core::orchestra::executor::Orchestrator;
use helheim_core::common::context::ExecutionContext;

#[tokio::test]
async fn test_deep_recursion_no_stackoverflow() {
    let mut script = String::new();
    for _ in 0..5000 {
        script.push_str("als waar {\n");
    }
    script.push_str("    retourneer \"success\";\n");
    for _ in 0..5000 {
        script.push_str("}\n");
    }
    
    let executor = Orchestrator::new(std::sync::Arc::new(helheim_core::network::DiscoveryService::new()));
    let mut ctx = ExecutionContext::default_privileged();
    ctx.current_module = Some("test".to_string());
    
    // Parse the script first
    let ast = match helheim_lang::parser::parse(&script) {
        Ok(ast) => ast,
        Err(_) => panic!("Failed to parse"),
    };
    
    let res: Option<String> = executor.execute_ast(ast, ctx).await.unwrap();
    assert_eq!(res, Some("success".to_string()));
}
