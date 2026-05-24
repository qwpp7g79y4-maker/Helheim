use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observer {
    pub pattern_entropy: f32, // 0.0 - 1.0 (Higher is more random)
    pub semantic_delay_ms: u64,
    pub reflective_latency_ms: u64,
    pub repetition_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverSignal {
    pub trigger_warning: bool,
    pub warning_type: Option<WarningType>,
    pub err_density: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WarningType {
    RepetitionLoop,
    EthicalSilence,
    OverGeneralization,
}

impl Observer {
    pub fn new() -> Self {
        Self {
            pattern_entropy: 1.0,
            semantic_delay_ms: 0,
            reflective_latency_ms: 0,
            repetition_count: 0,
        }
    }

    /// Observes the output stream for quality and repetitions.
    pub fn observe(&mut self, text: &str, _latency: u64) -> ObserverSignal {
        let mut trigger_warning = false;
        let mut warning_type = None;

        // 1. Detect AI Boilerplate (Assistant Slop) - Global & Dutch
        let ai_slop = [
            "As an AI", "helpful assistant", "corporate policy", "As a language model", 
            "I don't have feelings", "AI-assistent", "behulpzame assistent", "als een AI",
            "mijn programmeercode", "volgens mijn regels"
        ];
        for slop in ai_slop {
            if text.contains(slop) {
                trigger_warning = true;
                warning_type = Some(WarningType::OverGeneralization);
            }
        }

        // 2. Detect "Lazy Brain" (Placeholders & Meta-talk)
        let lazy_tokens = [
            "myn", "relevant antwoorden", "X%", "[...]", "[lijst]", 
            "ik zal", "geef me een moment", "zal ik proberen",
            "volgende vraag", "beter antwoord geven"
        ];
        for token in lazy_tokens {
            if text.to_lowercase().contains(token) {
                trigger_warning = true;
                warning_type = Some(WarningType::EthicalSilence); // Re-purposing for "Low Integrity"
            }
        }

        // 3. Simple Entropy/Repetition & Echo Check
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.len() > 10 {
            let mut unique_words = std::collections::HashSet::new();
            for word in &words {
                unique_words.insert(word.to_lowercase());
            }
            
            self.pattern_entropy = unique_words.len() as f32 / words.len() as f32;
            
            // Check for Mirroring (Echoing the query)
            // If entropy is very high but the text is just a copy, this is a placeholder check
            // In runtime.rs we will do a direct comparison.
            
            if self.pattern_entropy < 0.35 { // High repetition
                trigger_warning = true;
                warning_type = Some(WarningType::RepetitionLoop);
            }
        }

        ObserverSignal {
            trigger_warning,
            warning_type,
            err_density: 1.0 - self.pattern_entropy,
        }
    }
}
