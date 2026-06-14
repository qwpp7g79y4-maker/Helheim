use helheim_lang::ast::CodeTaal;
use helheim_lang::parser::HelParser;
use helheim_lang::synthesis::GeneralPtxGenerator;
use helheim_lang::semantic::SemanticAnalyzer;

fn parse_and_link(source: &str) -> Vec<CodeTaal> {
    let mut ast = HelParser::parse(source).expect("Parse failed");
    // Simuleer de CLI flow: semantic check direct na parse (linker logic tests can be separate)
    SemanticAnalyzer::analyze(&mut ast).expect("Semantic failed");
    ast
}

#[test]
fn test_pratt_math_and_vars() {
    let src = r#"
        zet a = 10;
        zet b = 5 * 2 + 3;
        zet c = a + b;
    "#;
    let linked = parse_and_link(src);
    let mut ptx_gen = GeneralPtxGenerator::new();
    let ptx = ptx_gen.lower_general(&CodeTaal::Block { statements: linked })
        .expect("Lowering failed");

    assert!(ptx.contains("main"), "Moet een entry point hebben");
    assert!(ptx.contains("add") || ptx.contains("mul"), "Moet rekenkundige PTX hebben");
    assert!(!ptx.contains("// Unhandled statement"), "Mag geen unhandled statements hebben");
}

#[test]
fn test_functions_and_return() {
    let src = r#"
        functie verdubbel a {
            geef_terug a * 2.0;
        }
        zet res = 21.0;
        zet res = roep_aan verdubbel res;
    "#;
    let linked = parse_and_link(src);
    let mut ptx_gen = GeneralPtxGenerator::new();
    let ptx = ptx_gen.lower_general(&CodeTaal::Block { statements: linked })
        .expect("Lowering failed");

    assert!(ptx.contains(".func") && ptx.contains("verdubbel"), "Functie moet aanwezig zijn");
    assert!(ptx.contains("ret"), "Moet return logica hebben");
    assert!(!ptx.contains("// Unhandled statement"), "Mag geen unhandled statements hebben");
}

#[test]
fn test_if_loop_and_control_flow() {
    let src = r#"
        zet i = 0.0;
        zet ok = 0.0;
        zolang i < 5.0 {
            zet i = i + 1.0;
        }
        als i > 3.0 dan {
            zet ok = 1.0;
        }
    "#;
    let linked = parse_and_link(src);
    let mut ptx_gen = GeneralPtxGenerator::new();
    let ptx = ptx_gen.lower_general(&CodeTaal::Block { statements: linked })
        .expect("Lowering failed");

    assert!(ptx.contains("bra") && ptx.contains("loop"), "Moet branching/loop labels hebben");
    assert!(ptx.contains("setp"), "Moet predicaat vergelijkingen hebben");
    assert!(!ptx.contains("// Unhandled statement"), "Mag geen unhandled statements hebben");
}

#[test]
fn test_pure_code_no_fallbacks() {
    let src = r#"
        zet x = 10.0 + 5.0 * 2.0;
        functie is_even n {
            geef_terug n > 0.0;
        }
        zet resultaat = roep_aan is_even x;
    "#;
    let linked = parse_and_link(src);
    let mut ptx_gen = GeneralPtxGenerator::new();
    let ptx = ptx_gen.lower_general(&CodeTaal::Block { statements: linked })
        .expect("Lowering failed");

    assert!(!ptx.contains("// Unhandled statement"), "Mag geen unhandled statements hebben");
    assert!(!ptx.contains("HOST_OP: INTERPRETER"), "Mag niet terugvallen op interpreter");
    assert!(ptx.contains("main"), "Moet een bruikbare entry hebben");
}
