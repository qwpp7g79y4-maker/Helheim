use helheim_core::orchestra::Orchestrator;
use helheim_core::common::context::ExecutionContext;
use helheim_lang::ast::{CodeTaal, LiteralValue};

#[tokio::test]
async fn test_deep_recursion_default_stack() {
    let mut current = CodeTaal::Block { statements: vec![CodeTaal::Return { value: Some(Box::new(CodeTaal::Literal(LiteralValue::String("success".to_string())))) }] };
    for _ in 0..5000 {
        current = CodeTaal::If {
            condition: Box::new(CodeTaal::Literal(LiteralValue::String("waar".to_string()))),
            then: Box::new(current),
            else_block: None,
        };
    }
    
    let executor = Orchestrator::new(std::sync::Arc::new(helheim_core::network::DiscoveryService::new()));
    let mut ctx = ExecutionContext::default_privileged();
    ctx.current_module = Some("test".to_string());
    
    let res: Option<String> = executor.execute_ast(vec![current], ctx).await.unwrap();
    assert_eq!(res, Some("success".to_string()));
}
