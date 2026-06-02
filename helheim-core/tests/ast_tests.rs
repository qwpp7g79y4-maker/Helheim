use helheim_core::orchestra::parser::HelParser;
use helheim_lang::ast::CodeTaal;

// Test if the parser correctly parses modulo without panicking
#[test]
fn test_parse_modulo() {
    let script = "zet UITKOMST = 10 % 3;";
    let ast_result = HelParser::parse(script);
    
    assert!(ast_result.is_ok(), "Parser should not fail on modulo");
    let ast = ast_result.unwrap();
    
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        CodeTaal::VarDef { name, value } => {
            assert_eq!(name, "UITKOMST");
            match &**value {
                CodeTaal::Op { left, op, right } => {
                    assert_eq!(op, "%");
                    if let CodeTaal::Literal(ref l) = **left { 
                        if let helheim_lang::ast::LiteralValue::Int(i) = l { assert_eq!(*i, 10); } else { panic!("Expected Int Literal 10"); }
                    } else { panic!("Expected Literal 10"); }
                    if let CodeTaal::Literal(ref r) = **right { 
                        if let helheim_lang::ast::LiteralValue::Int(i) = r { assert_eq!(*i, 3); } else { panic!("Expected Int Literal 3"); }
                    } else { panic!("Expected Literal 3"); }
                }
                _ => panic!("Expected Op"),
            }
        }
        _ => panic!("Expected VarDef for zet command"),
    }
}

// Test string formatting logic (Interpolation)
#[test]
fn test_string_interpolation_parser() {
    let script = "print \"Hallo $NAAM\";";
    let ast = HelParser::parse(script).expect("Parser failed");
    
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        CodeTaal::Print { message } => {
            assert_eq!(message, "\"Hallo $NAAM\"");
        }
        _ => panic!("Expected Print command"),
    }
}

// Test error handling: missing token should not panic but return Error
#[test]
fn test_parser_missing_token_no_panic() {
    // Missing value for append
    let script = "voeg_toe FRUIT ;";
    let result = HelParser::parse(script);
    
    assert!(result.is_err(), "Parser should return Err on missing token instead of panicking");
}

#[test]
fn test_parse_pipe() {
    let script = "voer uit ls -la | grep helheim;";
    let ast_result = HelParser::parse(script);
    
    assert!(ast_result.is_ok(), "Parser should not fail on pipe");
    let ast = ast_result.unwrap();
    
    assert_eq!(ast.len(), 1);
    match &ast[0] {
        CodeTaal::SysOp { command } => {
            assert_eq!(command, "voer uit ls -la | grep helheim");
        }
        _ => panic!("Expected SysOp for voer uit with pipe"),
    }
}
