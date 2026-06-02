use helheim_lang::parser::HelParser;
use helheim_lang::synthesis::KernelSynthesisEngine;
use helheim_lang::ast::CodeTaal;

#[test]
fn test_helheim_language_to_ptx() {
    let code = r#"
        gpu_kernel ssn_compute_layer #[workgroup(256)] (a: Tensor, b: Tensor, c: Tensor) {
            matrix_mma a, b, c;
        }
    "#;

    // 1. Parsing
    println!("--- STAP 1: PARSEN VAN HELHEIM CODE ---");
    let ast = HelParser::parse(code).expect("Failed to parse code");
    println!("{:#?}", ast);

    // 2. Synthese
    println!("\n--- STAP 2: BARE-METAL PTX SYNTHESE ---");
    if let CodeTaal::GpuKernel(ref kernel) = ast[0] {
        let ptx = KernelSynthesisEngine::synthesize_gpu_kernel(kernel).expect("PTX Synthesis failed");
        println!("{}", ptx);
        
        // Simpele verificatie dat het dynamische registers heeft toegewezen
        assert!(ptx.contains(".reg .b32 %r<16>;"));
        assert!(ptx.contains("mma.sync.aligned.m16n8k16.row.col.f32.f16.f16.f32"));
        assert!(ptx.contains("ldmatrix.sync.aligned.m8n8.x4.shared.b16"));
    } else {
        panic!("Verwachtte een GpuKernel in the AST!");
    }
}

#[test]
fn test_helheim_control_flow_to_ptx() {
    let code = r#"
        gpu_kernel control_flow_test () {
            zet x = 10.0;
            zet y = 5.0;
            als x > y dan {
                zet x = x - 1.0;
            } anders {
                zet x = x + 1.0;
            }
            zolang x > 0.0 {
                zet x = x - 1.0;
            }
        }
    "#;

    let ast = HelParser::parse(code).expect("Failed to parse code");
    
    if let CodeTaal::GpuKernel(ref kernel) = ast[0] {
        let ptx = KernelSynthesisEngine::synthesize_gpu_kernel(kernel).expect("PTX Synthesis failed");
        println!("{}", ptx);
        
        // Verifieer predicaten
        assert!(ptx.contains(".reg .pred %p"));
        
        // Verifieer comparisies (x > y => gt, x > 0.0 => gt)
        assert!(ptx.contains("setp.gt.f32"));
        
        // Verifieer branching
        assert!(ptx.contains("@!%p"));
        assert!(ptx.contains("bra ELSE_"));
        assert!(ptx.contains("bra END_IF_"));
        assert!(ptx.contains("LOOP_START_"));
        assert!(ptx.contains("bra LOOP_END_"));
    } else {
        panic!("Verwachtte een GpuKernel in the AST!");
    }
}

#[test]
fn test_io_keywords_and_semantic_and_general_ptx_lowering() {
    use helheim_lang::semantic::SemanticAnalyzer;

    // Statement forms (plan A)
    let code = r#"
        zet mijn_pad = "/tmp/test.txt";
        zet url_var = "https://example.com/data";
        haal "https://example.com/data";
        lees "/tmp/test.txt";
        schrijf "/tmp/out.txt" "hello dynamic";
        zet x = lees mijn_pad;
        zet y = haal url_var;
    "#;

    let ast = HelParser::parse(code).expect("I/O keywords must parse");
    assert!(ast.iter().any(|n| matches!(n, CodeTaal::HttpOp { .. })));
    assert!(ast.iter().any(|n| matches!(n, CodeTaal::FileOp { action, .. } if action == "read")));
    assert!(ast.iter().any(|n| matches!(n, CodeTaal::FileOp { action, .. } if action == "write")));

    // Semantic must give correct return types (plan B)
    let mut ast2 = ast.clone();
    SemanticAnalyzer::analyze(&mut ast2).expect("I/O type safety must pass");
    // (we don't have direct type query here, but no error == success)

    // General Block/FunctionDef now forces PtxGenerator (plan A in synthesis)
    let block = CodeTaal::Block { statements: vec![
        CodeTaal::VarDef { name: "a".into(), value: Box::new(CodeTaal::Literal(helheim_lang::ast::LiteralValue::Int(42))) },
        CodeTaal::Op { left: Box::new(CodeTaal::VarGet { name: "a".into() }), op: "+".into(), right: Box::new(CodeTaal::Literal(helheim_lang::ast::LiteralValue::Int(1))) },
    ]};
    let ptx = KernelSynthesisEngine::synthesize(block).expect("general block lowering must not panic");
    assert!(ptx.contains("hel_lowered") || ptx.contains("add.u32") || ptx.contains("mov.u32"), "should contain lowered PTX with int math");
}
