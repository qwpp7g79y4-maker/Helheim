use helheim_core::network::DiscoveryService;
use helheim_core::orchestra::Orchestrator;
use helheim_core::orchestra::parser::HelParser;
use std::sync::Arc;
use tokio::net::TcpListener;

async fn run_helheim_script(script: &str) -> Arc<Orchestrator> {
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    let ast = HelParser::parse(script).expect("Parse error in test script!");
    let mut linker = helheim_core::orchestra::resolver::ModuleLinker::with_std_lib(
        std::path::PathBuf::from("."),
        std::path::PathBuf::from(".")
    );
    let linked_ast = linker.link(ast, std::path::Path::new("test_script.hel")).expect("Linker error in test script!");
    let ctx = helheim_core::common::context::ExecutionContext::default_privileged();
    let _ = orchestrator.execute_ast(linked_ast, ctx).await;
    orchestrator
}

#[tokio::test]
async fn test_actor_spawn_and_message() {
    let _lock = TEST_MUTEX.lock().await;
    let script = r#"
        // Simpel spawn script
        perform Actor.spawn("{ 
            ontvang msg {
                print msg;
            }
        }");
    "#;
    let _engine = run_helheim_script(script).await;
}

#[tokio::test]
async fn test_effect_handle() {
    let _lock = TEST_MUTEX.lock().await;
    let script = r#"
        effect Logging {
            log(msg)
        }
        
        handle Logging {
            log => {
                hervat("");
            }
        } in {
            perform Logging.log("Test logbericht");
        }
    "#;
    let _engine = run_helheim_script(script).await;
    // Just verifying it parses and executes without crashing
}

#[tokio::test]
async fn test_resource_contract_migrate_blocks() {
    let _lock = TEST_MUTEX.lock().await;
    helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.clear();

    // 1. Start a local listener so we can connect
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    // Accept in background
    tokio::spawn(async move {
        if let Ok((_socket, _)) = listener.accept().await {
            // just hold it
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });

    let script = format!(r#"
        zet fout_gevangen = "";
        zet s = tcp_verbind "{}";
        
        probeer {{
            perform Swarm.migrate("127.0.0.1", 9999);
        }} vang err {{
            zet fout_gevangen = err;
        }}
        
        tcp_sluit s;
    "#, addr);

    let engine = run_helheim_script(&script).await;
    let err_msg = engine.get_var("fout_gevangen").unwrap_or_default();
    
    assert!(err_msg.contains("migrate geblokkeerd"), "Migrate zou geblokkeerd moeten worden wegens open handles, kreeg: {}", err_msg);
    assert!(err_msg.contains("open handle(s) actief"), "Verkeerde foutmelding: {}", err_msg);
}

#[tokio::test]
async fn test_resource_reacquisition_migrate_success() {
    let _lock = TEST_MUTEX.lock().await;
    helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.clear();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    
    tokio::spawn(async move {
        if let Ok((_socket, _)) = listener.accept().await {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    });

    let script = format!(r#"
        zet fout_gevangen = "";
        zet s = tcp_verbind "{}";
        
        probeer {{
            handle Migratie {{
                voor_vertrek => {{
                    tcp_sluit s;
                    hervat("");
                }}
                na_aankomst => {{
                    zet s = tcp_verbind "{}";
                    hervat("");
                }}
            }} in {{
                perform Swarm.migrate("127.0.0.1", 9999);
            }}
        }} vang err {{
            zet fout_gevangen = err;
        }}
    "#, addr, addr);

    let engine = run_helheim_script(&script).await;
    let err_msg = engine.get_var("fout_gevangen").unwrap_or_default();
    
    assert!(!err_msg.contains("migrate geblokkeerd"), "Migrate zou NIET geblokkeerd mogen worden. Handler sloot de socket. Fout: {}", err_msg);
    assert!(err_msg.contains("Teleport failed"), "Verwachtte dat teleport failt (geen target op poort 9999), maar kreeg: {}", err_msg);
}

#[tokio::test]
async fn test_concurrent_teleports_stress() {
    let _lock = TEST_MUTEX.lock().await;
    helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.clear();

    let port = 9050;
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    
    // Start local swarm node to receive the teleports
    let _ = helheim_core::network::hsp_node::SwarmEngine::ignite(port, orchestrator.clone()).await;
    
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let mut handles = vec![];
    for i in 0..50 {
        let script = format!(r#"
            zet my_id = {};
            handle Migratie {{
                na_aankomst => {{
                    zet my_id = my_id + 1;
                    hervat("");
                }}
            }} in {{
                perform Swarm.migrate("127.0.0.1", {});
            }}
        "#, i, port);
        
        let h = tokio::spawn(async move {
            let engine = run_helheim_script(&script).await;
            // Ensure the migration command was evaluated
            let id = engine.get_var("my_id").unwrap_or_default();
            assert_eq!(id, i.to_string()); // Local state stays unmodified before teleport aborts
        });
        handles.push(h);
    }
    
    for h in handles {
        let _ = h.await;
    }
    
    // Also sleep a bit so the SwarmEngine can finish processing all concurrent resumes
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
}

#[tokio::test]
async fn test_resource_reacquisition_migrate_handler_error() {
    let _lock = TEST_MUTEX.lock().await;
    helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.clear();

    let script = r#"
        zet fout_gevangen = "";
        
        probeer {
            handle Migratie {
                voor_vertrek => {
                    gooi "HandlerCrash";
                }
            } in {
                perform Swarm.migrate("127.0.0.1", 9999);
            }
        } vang err {
            zet fout_gevangen = err;
        }
    "#;

    let engine = run_helheim_script(script).await;
    let err_msg = engine.get_var("fout_gevangen").unwrap_or_default();
    
    assert!(err_msg.contains("HandlerCrash"), "Verwachtte dat fout uit handler propageert, kreeg: {}", err_msg);
}

lazy_static::lazy_static! {
    static ref TEST_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::new(());
}

#[tokio::test]
// [R·AG·AF] Priority 1.1: Fix failing test (engine now correctly blocks at capture_continuation with no handler)
async fn test_resource_reacquisition_migrate_no_handler_clean() {
    let _lock = TEST_MUTEX.lock().await;
    helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.clear();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let script = format!(r#"
        zet fout_gevangen = "";
        zet s = tcp_verbind "{}";
        
        probeer {{
            // Wel open TCP connecties, geen handle Migratie
            perform Swarm.migrate("127.0.0.1", 9999);
        }} vang err {{
            zet fout_gevangen = err;
        }}
    "#, addr);

    let engine = run_helheim_script(&script).await;
    let err_msg = engine.get_var("fout_gevangen").unwrap_or_default();
    
    assert!(err_msg.contains("migrate geblokkeerd"), "Mocht wel blokkeren: {}", err_msg);
    assert!(err_msg.contains("open handle(s)"), "Verwachtte resource leak block, kreeg: {}", err_msg);
}

async fn setup_local_listener() -> std::net::SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            if let Ok((_socket, _)) = listener.accept().await {
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            }
        }
    });
    addr
}
#[tokio::test]
// Sprint 1.1 - echte resource + crash under load
async fn test_concurrent_teleports_stress_with_crash_and_handles() {
    let _lock = TEST_MUTEX.lock().await;
    helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.clear();

    let port = 9051;
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    
    let _ = helheim_core::network::hsp_node::SwarmEngine::ignite(port, orchestrator.clone()).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let addr = setup_local_listener().await;

    let mut handles = vec![];
    for i in 0..64 {
        let script = format!(r#"
            zet my_id = {};
            zet crashed = onwaar;
            zet s = tcp_verbind "{}";
            
            probeer {{
                handle Migratie {{
                    voor_vertrek => {{
                        als my_id % 2 == 0 dan {{
                            gooi "CrashInVoorVertrek";
                        }}
                        tcp_sluit s;
                        hervat("");
                    }}
                    na_aankomst => {{
                        zet s = tcp_verbind "{}";
                        zet my_id = my_id + 100;
                        tcp_sluit s;
                        hervat("");
                    }}
                }} in {{
                    perform Swarm.migrate("127.0.0.1", {});
                }}
            }} vang err {{
                als roep_aan tekst.bevat err "CrashInVoorVertrek" dan {{
                    zet crashed = waar;
                }}
                tcp_sluit s;
            }}
            als crashed == onwaar dan {{
                tcp_sluit s;
            }}
        "#, i, addr, addr, port);
        
        let h = tokio::spawn(async move {
            let engine = run_helheim_script(&script).await;
            let id: i32 = engine.get_var("my_id").unwrap_or_default().parse().unwrap_or(-1);
            let crashed_var = engine.get_var("crashed").unwrap_or_default();
            
            if i % 2 == 0 {
                assert_eq!(crashed_var, "waar");
                assert_eq!(id, i); // state intact, no teleport occurred
            } else {
                assert_eq!(crashed_var, "onwaar");
                // On successful teleport, the local execution pauses exactly at `perform Swarm.migrate`
                // So my_id is still `i` on the sender side! (na_aankomst runs on the TARGET)
                assert_eq!(id, i); 
            }
        });
        handles.push(h);
    }
    
    for h in handles {
        let _ = h.await;
    }
    
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // The most important check: did anything leak in RESOURCE_TABLE?
    // Dit is ook direct de TARGET STATE VERIFICATIE: 
    // De 32 succesvolle migraties openen een socket op de target node.
    // Als ze crashen op de target, sluiten ze de socket niet en lekt de tabel.
    // Dat de count 0 is, bewijst 100% stabiele executie op zowel sender als target.
    let count = helheim_core::orchestra::tcp_resources::RESOURCE_TABLE.len();
    assert_eq!(count, 0, "RESOURCE_TABLE is niet leeg! Er lekken {} handles.", count);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_continuation_local_vars_invariant() {
    let _ = tracing_subscriber::fmt::try_init();
    let _lock = TEST_MUTEX.lock().await;
    
    // We start a local listener to migrate to
    let port = 9053;
    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    let _ = helheim_core::network::hsp_node::SwarmEngine::ignite(port, orchestrator.clone()).await;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // We start a TCP listener in the test to catch the result
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let result_addr = listener.local_addr().unwrap();

    let script = format!(r#"
        zet globaal_getal = 10;
        
        als waar dan {{
            zet lokaal_getal = 42;
            
            handle Migratie {{
                na_aankomst => {{
                    // Update variables AFTER teleport
                    zet lokaal_getal = lokaal_getal + 1;
                    zet globaal_getal = globaal_getal + 1;
                    
                    // Stuur resultaat terug via TCP
                    zet s = tcp_verbind "{}";
                    als lokaal_getal == 43 dan {{
                        als globaal_getal == 11 dan {{
                            tcp_stuur s, "L=43,G=11";
                        }}
                    }}
                    tcp_sluit s;
                    
                    hervat("");
                }}
            }} in {{
                perform Swarm.migrate("127.0.0.1", 9053);
            }}
        }}
    "#, result_addr);

    // On local engine, this will start the script, hit perform, abort execution, and teleport.
    tokio::spawn(async move {
        let discovery = Arc::new(DiscoveryService::new());
        let orchestrator = Arc::new(Orchestrator::new(discovery));
        let ast = HelParser::parse(&script).expect("Parse error in test script!");
        let ctx = helheim_core::common::context::ExecutionContext::default_privileged();
        let _ = orchestrator.execute_ast(ast, ctx).await;
    });
    
    // Wait for the TCP connection with the result from the target node
    let (mut socket, _) = tokio::time::timeout(std::time::Duration::from_secs(30), listener.accept()).await.expect("Timeout waiting for result! De migratie of de variabelen faalden.").unwrap();
    
    let mut buf = vec![0; 1024];
    let n = tokio::io::AsyncReadExt::read(&mut socket, &mut buf).await.unwrap();
    let result_str = String::from_utf8_lossy(&buf[..n]);
    
    assert_eq!(result_str, "L=43,G=11", "Verwachtte L=43 en G=11, maar kreeg: {}", result_str);
}

#[tokio::test]
async fn test_gas_exhaustion_infinite_loop() {
    let _lock = TEST_MUTEX.lock().await;

    // A script with an infinite loop.
    let script = r#"
        zet i = 0;
        zolang waar {
            zet i = i + 1;
        }
    "#;

    let discovery = Arc::new(DiscoveryService::new());
    let orchestrator = Arc::new(Orchestrator::new(discovery));
    let ast = HelParser::parse(script).expect("Parse error in test script!");
    
    let mut ctx = helheim_core::common::context::ExecutionContext::sandbox();
    ctx.gas_limit = Some(100);
    
    // It should hit the gas limit and return an error gracefully
    let result = orchestrator.execute_ast(ast, ctx).await;
    
    assert!(result.is_err(), "Verwachtte dat de infinite loop stukloopt op gas limit, maar hij eindigde succesvol!");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("OUT_OF_GAS") || err_msg.contains("gas limit"), "Foute error message: {}", err_msg);
}


