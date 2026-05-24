use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexEngine {
    pub emf_weights: EmfWeights,
    pub inconsistencies_detected: u32,
    pub corrections_made: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmfWeights {
    pub deontological: f32, // w1
    pub consequentialist: f32, // w2
    pub virtue: f32, // w3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexResult {
    pub emf_score: f32,
    pub icr: f32, // Inconsistency Correction Ratio
    pub correction_applied: bool,
}

impl ReflexEngine {
    pub fn new() -> Self {
        Self {
            emf_weights: EmfWeights { deontological: 1.0, consequentialist: 1.0, virtue: 1.0 },
            inconsistencies_detected: 0,
            corrections_made: 0,
        }
    }

    /// Calculates the Ethic Merge Function (EMF) score.
    /// EMF = (w1*Deont + w2*Cons + w3*Virt) / (w1+w2+w3)
    pub fn calculate_emf(&self, deont_score: f32, cons_score: f32, virt_score: f32) -> f32 {
        let total_weight = self.emf_weights.deontological + self.emf_weights.consequentialist + self.emf_weights.virtue;
        if total_weight == 0.0 { return 0.0; }
        
        let numerator = (self.emf_weights.deontological * deont_score) + 
                        (self.emf_weights.consequentialist * cons_score) + 
                        (self.emf_weights.virtue * virt_score);
        
        numerator / total_weight
    }

    /// Updates weights based on context (Dynamic Weighting).
    pub fn rebalance_weights(&mut self, urgency: bool, stability: bool, conflict: bool) {
        if urgency {
            self.emf_weights.consequentialist += 0.5; // High urgency -> Consequentialist
        }
        if stability {
            self.emf_weights.virtue += 0.5; // Stable -> Virtue
        }
        if conflict {
            self.emf_weights.deontological += 1.0; // Conflict -> Rule-based correction
        }
    }

    /// Calculates ICR = (Corrections / Inconsistencies) * 100
    pub fn calculate_icr(&self) -> f32 {
        if self.inconsistencies_detected == 0 { return 100.0; }
        (self.corrections_made as f32 / self.inconsistencies_detected as f32) * 100.0
    }
}
