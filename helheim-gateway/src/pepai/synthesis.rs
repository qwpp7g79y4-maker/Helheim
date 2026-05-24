use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptualBridge {
    pub source_id: String,
    pub target_id: String,
    pub shared_concept: String,
    pub strength: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisEngine {
    pub active_bridges: Vec<ConceptualBridge>,
    pub intuition_threshold: f32,
}

impl SynthesisEngine {
    pub fn new() -> Self {
        Self {
            active_bridges: Vec::new(),
            intuition_threshold: 0.7,
        }
    }

    /// Detects bridges between retrieved memories based on shared keywords or vector clusters.
    /// (Simplified version: using string overlap until vector logic is expanded).
    pub fn detect_bridges(&mut self, memories: &[(String, String)]) {
        self.active_bridges.clear();
        
        for i in 0..memories.len() {
            for j in i + 1..memories.len() {
                let (id_a, content_a) = &memories[i];
                let (id_b, content_b) = &memories[j];
                
                // Look for shared "heavy" keywords (length > 5)
                let set_a: std::collections::HashSet<_> = content_a
                    .split_whitespace()
                    .filter(|s| s.len() > 5)
                    .collect();
                let set_b: std::collections::HashSet<_> = content_b
                    .split_whitespace()
                    .filter(|s| s.len() > 5)
                    .collect();
                
                let intersection: Vec<_> = set_a.intersection(&set_b).collect();
                
                if !intersection.is_empty() {
                    self.active_bridges.push(ConceptualBridge {
                        source_id: id_a.clone(),
                        target_id: id_b.clone(),
                        shared_concept: intersection[0].to_string(),
                        strength: (intersection.len() as f32 / set_a.len().max(1) as f32) * 2.0,
                    });
                }
            }
        }
    }

    pub fn get_synthesis_prompt(&self) -> String {
        if self.active_bridges.is_empty() {
            return "Geen directe bruggen gevonden tussen herinneringen. Focus op directe analyse.".to_string();
        }

        let mut prompt = String::from("BRUG-SYNTHESE ACTIEF:\n");
        for bridge in &self.active_bridges {
            prompt.push_str(&format!(
                "- Je ziet een link tussen '{}' en '{}' via het concept '{}'. Smeed dit in je antwoord.\n",
                bridge.source_id, bridge.target_id, bridge.shared_concept
            ));
        }
        prompt.push_str("Dwing jezelf om een 'Aha!'-moment te creëren door deze verbanden te leggen.");
        prompt
    }
}
