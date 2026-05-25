use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use std::sync::Arc;

// --- Helper Functie ---
// Dit simuleert een volledige, geïsoleerde Helheim execution environment in het RAM
async fn run_helheim_script(script: &str) -> Arc<Orchestrator> {
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));

    let ast = HelParser::parse(script).expect("Parse error in test script!");
    orchestrator
        .execute_ast(ast)
        .await
        .expect("Runtime error in test script!");

    orchestrator
}

#[tokio::test]
async fn test_math_operations() {
    let script = r#"
        zet a = 10;
        zet b = 25;
        zet c = a + b;
        zet d = c * 2;
    "#;

    let engine = run_helheim_script(script).await;

    assert_eq!(engine.get_var("a").unwrap(), "10", "Variabele a faalt");
    assert_eq!(engine.get_var("b").unwrap(), "25", "Variabele b faalt");
    assert_eq!(engine.get_var("c").unwrap(), "35", "Optelling faalt");
    assert_eq!(
        engine.get_var("d").unwrap(),
        "70",
        "Vermenigvuldiging faalt"
    );
}

#[tokio::test]
async fn test_array_mutations() {
    let script = r#"
        zet lijst = [1, 2, 3];
        voeg_toe lijst 4;
        zet lengte_lijst = lengte(lijst);
        zet eerste = lijst[0];
    "#;

    let engine = run_helheim_script(script).await;

    // Test Array Appending
    let json_array = engine.get_var("lijst").unwrap();
    assert!(
        json_array.contains("1") && json_array.contains("4"),
        "Array mutatie faalt: {}",
        json_array
    );

    // Test Length function
    assert_eq!(
        engine.get_var("lengte_lijst").unwrap(),
        "4",
        "Lengte functie faalt"
    );

    // Test Array Indexing
    assert_eq!(
        engine.get_var("eerste").unwrap(),
        "1",
        "Array indexering faalt"
    );
}

#[tokio::test]
async fn test_concurrency_block() {
    let script = r#"
        zet voor_parallel = 1;
        tegelijkertijd {
            roep_aan wacht 1;
            roep_aan wacht 1;
        }
        zet na_parallel = 2;
    "#;

    // Start stopwatch
    let start = std::time::Instant::now();
    let engine = run_helheim_script(script).await;
    let duration = start.elapsed();

    assert_eq!(engine.get_var("voor_parallel").unwrap(), "1");
    assert_eq!(engine.get_var("na_parallel").unwrap(), "2");

    // Omdat de 2 wacht commando's (ieder 1s) tegelijk starten, moet het totale script ~1 seconde duren, niet 2.
    // We geven een ruime marge (max 1.5s) om trage CI runners niet te laten flaken.
    assert!(
        duration.as_secs_f64() < 1.5,
        "Concurrency gefaald, duurde {} seconden",
        duration.as_secs_f64()
    );
}

#[tokio::test]
async fn test_if_else_logic() {
    let script = r#"
        zet leeftijd = 20;
        zet volwassen = 0;
        
        als leeftijd > 18 dan {
            zet volwassen = 1;
        } anders {
            zet volwassen = 0;
        }
    "#;

    let engine = run_helheim_script(script).await;
    assert_eq!(
        engine.get_var("volwassen").unwrap(),
        "1",
        "Logische operator faalt"
    );
}

#[tokio::test]
async fn test_error_handling() {
    let script = r#"
        zet foutmelding = "";
        probeer {
            gooi "TestFout";
        } vang err {
            zet foutmelding = err;
        }
    "#;

    let engine = run_helheim_script(script).await;
    let fout = engine.get_var("foutmelding").unwrap();
    assert!(
        fout.contains("TestFout"),
        "Foutmelding is niet gevangen: {}",
        fout
    );
}

#[tokio::test]
async fn test_import_module() {
    let math_module = r#"
        functie bereken_btw bedrag {
            geef_terug bedrag * 1.21;
        }
        zet wiskunde_versie = 1.0;
    "#;
    let module_path = "test_module.hel";
    tokio::fs::write(module_path, math_module).await.unwrap();

    let script = r#"
        gebruik "test_module.hel";
        zet start_bedrag = 100;
        zet inclusief_btw = roep_aan bereken_btw start_bedrag;
    "#;

    let engine = run_helheim_script(script).await;

    // Clean up
    let _ = tokio::fs::remove_file(module_path).await;

    // Check if the variable and function from the imported module are available
    assert_eq!(
        engine.get_var("wiskunde_versie").unwrap(),
        "1",
        "Variabele uit module niet ingeladen"
    );
    assert_eq!(
        engine.get_var("inclusief_btw").unwrap(),
        "121",
        "Functie uit module faalde"
    );
}

#[tokio::test]
async fn test_models() {
    let script = r#"
        model Persoon { naam, leeftijd, actief }
        zet werknemer = nieuw Persoon("Pieter", 30, waar);
        zet info = werknemer["naam"];
    "#;
    
    let engine = run_helheim_script(script).await;
    
    // De output JSON checken
    let raw_json = engine.get_var("werknemer").unwrap();
    assert!(raw_json.contains("\"naam\":\"Pieter\""), "Naam faalde in model");
    assert!(raw_json.contains("\"leeftijd\":30.0"), "Leeftijd faalde in model: {}", raw_json);
    assert!(raw_json.contains("\"actief\":true"), "Boolean faalde in model: {}", raw_json);
    
    // Check de extractie via haken syntax
    assert_eq!(engine.get_var("info").unwrap(), "Pieter", "Uitlezen uit model faalde");
}

#[tokio::test]
async fn test_stdlib() {
    let script = r#"
        zet bron = "Hallo Helheim Wereld";
        zet l = roep_aan tekst.lengte bron;
        zet vervangen = roep_aan tekst.vervang bron "Wereld" "Matrix";
        zet caps = roep_aan tekst.hoofdletters bron;
        
        zet rnd = roep_aan wiskunde.willekeurig 1 1;
        zet afgerond = roep_aan wiskunde.afronden "3.7";
    "#;
    
    let engine = run_helheim_script(script).await;
    
    assert_eq!(engine.get_var("l").unwrap(), "20");
    assert_eq!(engine.get_var("vervangen").unwrap(), "Hallo Helheim Matrix");
    assert_eq!(engine.get_var("caps").unwrap(), "HALLO HELHEIM WERELD");
    assert_eq!(engine.get_var("rnd").unwrap(), "1");
    assert_eq!(engine.get_var("afgerond").unwrap(), "4");
}
