use helheim_lang::ast::CodeTaal;

/// Core effects die Helheim kent.
/// Deze worden automatisch geregistreerd bij bootstrap.
pub const CORE_EFFECTS: &[(&str, &[&str])] = &[
    ("Tcp", &["verbind", "luister", "accepteer", "stuur", "ontvang", "sluit"]),
    ("Actor", &["spawn", "send", "receive"]),
    ("Ffi", &["call"]),
    ("Trace", &["record"]),           // Flight Recorder
    ("Asm", &["inline"]),             // Inline PTX/ASM
    ("Swarm", &["dispatch", "migrate"]),         // Cross-node ECDH Teleportation & Self-Healing
    ("Migratie", &["voor_vertrek", "na_aankomst"]),  // Automatic re-acquisition hooks around Swarm.migrate
];

/// Maakt een Perform node voor een core effect (handig in executor of code-gen).
pub fn perform_tcp(op: &str, args: Vec<CodeTaal>) -> CodeTaal {
    CodeTaal::Perform {
        effect: "Tcp".to_string(),
        operation: op.to_string(),
        args,
    }
}

pub fn perform_actor(op: &str, args: Vec<CodeTaal>) -> CodeTaal {
    CodeTaal::Perform {
        effect: "Actor".to_string(),
        operation: op.to_string(),
        args,
    }
}

pub fn perform_migratie(op: &str) -> CodeTaal {
    CodeTaal::Perform {
        effect: "Migratie".to_string(),
        operation: op.to_string(),
        args: vec![],
    }
}
