use helheim_core::orchestra::Orchestrator;
use helheim_core::common::context::ExecutionContext;
use helheim_lang::ast::{CodeTaal, LiteralValue};

#[tokio::test]
async fn test_deep_recursion_no_stackoverflow() {
    let handle = std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
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
            })
        }).unwrap();
    handle.join().unwrap();
}

#[tokio::test]
async fn test_stack_overflow_error_clean() {
    let handle = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                let mut current = CodeTaal::Block { statements: vec![CodeTaal::Return { value: Some(Box::new(CodeTaal::Literal(LiteralValue::String("success".to_string())))) }] };
                for _ in 0..15000 {
                    current = CodeTaal::If {
                        condition: Box::new(CodeTaal::Literal(LiteralValue::String("waar".to_string()))),
                        then: Box::new(current),
                        else_block: None,
                    };
                }
                
                let executor = Orchestrator::new(std::sync::Arc::new(helheim_core::network::DiscoveryService::new()));
                let mut ctx = ExecutionContext::default_privileged();
                ctx.current_module = Some("test".to_string());
                
                let res: anyhow::Result<Option<String>> = executor.execute_ast(vec![current], ctx).await;
                assert!(res.is_err(), "Expected stack overflow error");
                let err_msg = res.unwrap_err().to_string();
                assert!(err_msg.contains("StackOverflow"), "Expected StackOverflow error, got: {}", err_msg);
            })
        }).unwrap();
    handle.join().unwrap();
}
