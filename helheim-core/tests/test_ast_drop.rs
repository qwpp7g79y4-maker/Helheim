use helheim_lang::ast::{CodeTaal, LiteralValue};

#[tokio::test]
async fn test_ast_drop_stack_overflow_15k() {
    let mut current = CodeTaal::Block { statements: vec![CodeTaal::Literal(LiteralValue::String("success".to_string()))] };
    for _ in 0..15000 {
        current = CodeTaal::If {
            condition: Box::new(CodeTaal::Literal(LiteralValue::String("waar".to_string()))),
            then: Box::new(current),
            else_block: None,
        };
    }
    // Drop it!
}
