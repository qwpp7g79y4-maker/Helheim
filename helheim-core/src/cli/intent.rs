use regex::Regex;

#[derive(Debug, PartialEq)]
pub enum Intent {
    // Actionable Intents
    Send { target: String, payload: String },
    SetVar { name: String, value: String },
    MatMul { size: usize }, // "matmul 2048"

    // Diagnostic Intents
    Diagnosis, // "wat is er mis", "status", "foutcodes"
    Fix,       // "los op", "heractiveer"
    Speed,     // "sneller", "performance"
    Update,    // "welke updates", "is er iets nieuws"
    Research,  // "zoek dit uit", "analyseer"

    Unknown,
}

pub struct IntentParser;

impl IntentParser {
    /// Vertaalt 'Pieter-Taal' (Chaotisch/Direct) naar Systeem Intentie.
    pub fn parse(input: &str) -> Intent {
        let normalized = input.trim(); // Keep case for payload, regex with case-insensitive flag handles commands

        // 1. Check Fuzzy Send: "Ey stuur 'hallo' even naar server"
        // Regex: (stuur|zend) <payload> (naar|aan) <target>
        // Note: (?i) makes it case insensitive
        let re_send = Regex::new(r"(?i)(?:stuur|zend)\s+(.+)\s+(?:naar|aan)\s+(.+)").unwrap();
        if let Some(caps) = re_send.captures(normalized) {
            let payload = caps
                .get(1)
                .map_or("", |m| m.as_str())
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .to_string();
            let target = caps
                .get(2)
                .map_or("", |m| m.as_str())
                .trim()
                .trim_matches(';')
                .to_string(); // Strip optional trailing ;
            return Intent::Send { target, payload };
        }

        // 2. Check Fuzzy Set: "Zet x op 10" or "Maak x gelijk aan 10"
        let re_set = Regex::new(r"(?i)(?:zet|maak)\s+(\w+)\s+(?:op|gelijk aan|=)\s+(.+)").unwrap();
        if let Some(caps) = re_set.captures(normalized) {
            let name = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let value = caps
                .get(2)
                .map_or("", |m| m.as_str())
                .trim()
                .trim_matches(';')
                .to_string();
            return Intent::SetVar { name, value };
        }

        // 3. Matrix Kernels: "matmul 2048"
        let re_matmul = Regex::new(r"(?i)matmul\s+(\d+)").unwrap();
        if let Some(caps) = re_matmul.captures(normalized)
            && let Ok(size) = caps[1].parse::<usize>() {
                return Intent::MatMul { size };
            }

        // 3. Simple Keyword Matching
        let lower = normalized.to_lowercase();

        if lower.contains("update") || lower.contains("nieuws") || lower.contains("versie") {
            return Intent::Update;
        }

        if lower.contains("zoek")
            || lower.contains("analyse")
            || lower.contains("waarom")
            || lower.contains("uitzoeken")
        {
            return Intent::Research;
        }

        if lower.contains("los op")
            || lower.contains("heractiveer")
            || lower.contains("maak")
            || lower.contains("fix")
        {
            return Intent::Fix;
        }

        if lower.contains("snel") || lower.contains("traag") || lower.contains("boost") {
            return Intent::Speed;
        }

        if lower.contains("wat")
            || lower.contains("hoe")
            || lower.contains("status")
            || lower.contains("info")
            || lower.contains("fout")
        {
            return Intent::Diagnosis;
        }

        Intent::Unknown
    }
}
