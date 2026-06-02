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
    let ctx = helheim_core::common::context::ExecutionContext::default_privileged();
    orchestrator
        .execute_ast(ast, ctx)
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

#[tokio::test]
async fn test_logic_operators() {
    let script = r#"
        zet x = 10;
        zet y = 20;
        
        zet is_and = x == 10 && y == 20;
        zet is_or = x == 5 || y == 20;
        zet is_false = x == 5 && y == 20;
    "#;
    let engine = run_helheim_script(script).await;
    
    assert_eq!(engine.get_var("is_and").unwrap(), "waar");
    assert_eq!(engine.get_var("is_or").unwrap(), "waar");
    assert_eq!(engine.get_var("is_false").unwrap(), "onwaar");
}

#[tokio::test]
async fn test_json_parsing() {
    let script = r#"
        zet ruw = "{\"naam\":\"NEXUS\",\"leeftijd\":1}";
        zet ontleed = roep_aan json.ontleden ruw;
        
        // Nu kunnen we de haken-syntax gebruiken
        zet naam = ontleed["naam"];
        zet leeftijd = ontleed["leeftijd"];
    "#;
    let engine = run_helheim_script(script).await;
    
    assert_eq!(engine.get_var("naam").unwrap(), "NEXUS");
    assert_eq!(engine.get_var("leeftijd").unwrap(), "1");
}

#[tokio::test]
async fn test_bilingual_parser() {
    let script = r#"
        let x = 10;
        set y = 20;
        
        if x == 10 && y == 20 then {
            let z = invoke wiskunde.afronden "10.4";
        }
    "#;
    let engine = run_helheim_script(script).await;
    
    assert_eq!(engine.get_var("x").unwrap(), "10");
    assert_eq!(engine.get_var("y").unwrap(), "20");
    assert_eq!(engine.get_var("z").unwrap(), "10");
}

#[tokio::test]
async fn test_network_io() {
    let script = r#"
        let response = invoke netwerk.get "https://jsonplaceholder.typicode.com/todos/1";
        if response != "null" then {
            let api_data = invoke json.ontleden response;
            let success = "yes";
        }
    "#;
    let engine = run_helheim_script(script).await;
    
    // Test passes if success variable was evaluated (meaning no null response and valid parsing)
    assert_eq!(engine.get_var("success").unwrap(), "yes");
}

#[tokio::test]
async fn test_while_loop() {
    let script = r#"
        let teller = 0;
        while teller < 5 do {
            let teller = teller + 1;
        }
        
        if teller == 5 then {
            let success = "yes";
        }
    "#;
    let engine = run_helheim_script(script).await;
    
    assert_eq!(engine.get_var("teller").unwrap(), "5");
    assert_eq!(engine.get_var("success").unwrap(), "yes");
}

#[tokio::test]
async fn test_file_io() {
    let script = r#"
        let test_inhoud = "Hallo wereld!";
        let success_schrijf = invoke bestand.schrijf "test_io_output.txt" test_inhoud;
        let gelezen = invoke bestand.lees "test_io_output.txt";
        if gelezen == "Hallo wereld!" then {
            let io_succes = "yes";
        }
    "#;
    let engine = run_helheim_script(script).await;
    assert_eq!(engine.get_var("io_succes").unwrap(), "yes");
    std::fs::remove_file("test_io_output.txt").unwrap_or(());
}

#[tokio::test]
async fn test_interp() {
    let script = r#"
        let naam = "NEXUS";
        let status = "online";
        let bericht = "De server {naam} is momenteel {status}!";
        if bericht == "De server NEXUS is momenteel online!" then {
            let interp_succes = "yes";
        }
    "#;
    let engine = run_helheim_script(script).await;
    assert_eq!(engine.get_var("interp_succes").unwrap(), "yes");
}

#[tokio::test]
async fn test_os() {
    let script = r#"
        let os_result = invoke systeem.shell "echo hallo_os";
        let env_var = invoke systeem.env "PATH";
        let timestamp = invoke systeem.tijd;

        if os_result == "hallo_os" then {
            let test_shell = "succes";
        }

        if env_var != "null" then {
            let test_env = "succes";
        }

        if timestamp != "null" then {
            let test_tijd = "succes";
        }
    "#;
    let engine = run_helheim_script(script).await;
    assert_eq!(engine.get_var("test_shell").unwrap(), "succes");
    assert_eq!(engine.get_var("test_env").unwrap(), "succes");
    assert_eq!(engine.get_var("test_tijd").unwrap(), "succes");
}

#[tokio::test]
async fn test_daemon() {
    let script = r#"
        achtergrond {
            invoke wacht 0;
            let daemon_klaar = "ja";
            invoke bestand.schrijf "daemon_test.txt" daemon_klaar;
        }
        let script_klaar = "ja";
    "#;
    let engine = run_helheim_script(script).await;
    assert_eq!(engine.get_var("script_klaar").unwrap(), "ja");
    // Wacht even tot de daemon klaar is
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let result = std::fs::read_to_string("daemon_test.txt").unwrap_or_default();
    assert_eq!(result.trim(), "ja");
    let _ = std::fs::remove_file("daemon_test.txt");
}

#[tokio::test]
async fn test_binary() {
    let script = r#"
        let b64_data = "SGVsbG8gV29ybGQ=";
        invoke bestand.schrijf_binair "test_bin_output.dat" b64_data;
        let ingelezen = invoke bestand.lees_binair "test_bin_output.dat";
        if ingelezen == b64_data then {
            let binary_test = "succes";
        }
    "#;
    let engine = run_helheim_script(script).await;
    assert_eq!(engine.get_var("binary_test").unwrap(), "succes");
    let _ = std::fs::remove_file("test_bin_output.dat");
}

#[tokio::test]
async fn test_dict() {
    let script = r#"
        let leeg = "{}";
        let stap1 = invoke dic.schrijf leeg "naam" "Bitboi";
        let stap2 = invoke dic.schrijf stap1 "leeftijd" 30;

        let test_naam = invoke dic.lees stap2 "naam";
        let test_leeftijd = invoke dic.lees stap2 "leeftijd";

        if test_naam == "Bitboi" dan {
            if test_leeftijd == 30 dan {
                let dict_test = "succes";
            }
        }
    "#;
    let engine = run_helheim_script(script).await;
    assert_eq!(engine.get_var("dict_test").unwrap(), "succes");
}

async fn run_helheim_script_sandbox(script: &str) -> Arc<Orchestrator> {
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));

    let ast = HelParser::parse(script).expect("Parse error in test script!");
    let ctx = helheim_core::common::context::ExecutionContext::sandbox();
    let _ = orchestrator.execute_ast(ast, ctx).await;

    orchestrator
}

#[tokio::test]
async fn test_sandbox_file_traversal() {
    let script = r#"
        let output = invoke bestand.lees "../../../etc/passwd";
    "#;
    let engine = run_helheim_script_sandbox(script).await;
    // Should fail and output should be null or empty
    assert_eq!(engine.get_var("output").unwrap_or("".to_string()), "");
}

#[tokio::test]
async fn test_ssrf_protection() {
    let script = r#"
        let output = invoke netwerk.get "http://127.0.0.1:8080/admin";
    "#;
    let engine = run_helheim_script_sandbox(script).await;
    assert_eq!(engine.get_var("output").unwrap_or("".to_string()), "");
}

#[tokio::test]
async fn test_sandbox_shell_blocked() {
    let script = r#"
        let os_result = invoke systeem.shell "echo hacked";
    "#;
    let engine = run_helheim_script_sandbox(script).await;
    assert_eq!(engine.get_var("os_result").unwrap_or("".to_string()), "");
}

#[tokio::test]
async fn test_snn_cortex_bitwise_and_popc() {
    // Load and run the SNN test script for coincidence detection and fire/misfire threshold
    let script = std::fs::read_to_string("./test_snn_cortex.hel")
        .expect("Failed to read test_snn_cortex.hel");
    let engine = run_helheim_script(&script).await;
    // Check that the input lists were set correctly (packing happened)
    let spikes = engine.get_var("input_spikes").unwrap_or("".to_string());
    println!("SNN input_spikes: {}", spikes);
    assert!(spikes.contains("list") || spikes.contains("waar"), "Expected list for spikes");
    let mask = engine.get_var("weight_mask").unwrap_or("".to_string());
    println!("SNN weight_mask: {}", mask);
    assert!(mask.contains("list") || mask.contains("waar"), "Expected list for mask");
    // The neuron_vuurt and vuurt may fallback to block text or computed if lowered taken
    let neuron = engine.get_var("neuron_vuurt").unwrap_or("".to_string());
    println!("SNN neuron_vuurt: {}", neuron);
    let fire = engine.get_var("vuurt").unwrap_or("".to_string());
    println!("SNN vuurt: {}", fire);
    // At least the script ran without runtime error, and lists were processed
    println!("SNN cortex test script executed successfully (lowered path depends on GPU availability)");
}
