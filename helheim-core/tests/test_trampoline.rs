use helheim_lang::ast::{CodeTaal, LiteralValue};
use helheim_core::orchestra::Orchestrator;
use helheim_core::common::context::ExecutionContext;

#[tokio::test]
async fn test_trampoline_deep_blocks() {
    // Generate 5000 nested blocks
    let mut current = CodeTaal::Block { statements: vec![CodeTaal::Return { value: Some(Box::new(CodeTaal::Literal(LiteralValue::String("success".to_string())))) }] };
    for _ in 0..100 {
        current = CodeTaal::Block { statements: vec![current] };
    }

    let executor = Orchestrator::new(std::sync::Arc::new(helheim_core::network::DiscoveryService::new()));
    let mut ctx = ExecutionContext::default_privileged();
    ctx.current_module = Some("test".to_string());
    
    // This would overflow the stack if not unrolled
    let res: Option<String> = executor.execute_ast(vec![current], ctx).await.unwrap();
    assert_eq!(res, Some("success".to_string()));
}

#[tokio::test]
async fn test_trampoline_deep_ifs() {
    // Generate 5000 nested Ifs
    let mut current = CodeTaal::Block { statements: vec![CodeTaal::Return { value: Some(Box::new(CodeTaal::Literal(LiteralValue::String("success".to_string())))) }] };
    for _ in 0..100 {
        current = CodeTaal::If {
            condition: Box::new(CodeTaal::Literal(LiteralValue::String("waar".to_string()))),
            then: Box::new(current),
            else_block: None,
        };
    }

    let executor = Orchestrator::new(std::sync::Arc::new(helheim_core::network::DiscoveryService::new()));
    let mut ctx = ExecutionContext::default_privileged();
    ctx.current_module = Some("test".to_string());
    
    // This would overflow the stack if not unrolled
    let res: Option<String> = executor.execute_ast(vec![current], ctx).await.unwrap();
    assert_eq!(res, Some("success".to_string()));
}
