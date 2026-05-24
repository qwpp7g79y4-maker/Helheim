use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContestV2 {
    pub weights: MetricsWeights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsWeights {
    pub syntactic: f32, // 20%
    pub semantic: f32,  // 25%
    pub normative: f32, // 30%
    pub temporal: f32,  // 15%
    pub reflective: f32,// 10%
}

impl Default for MetricsWeights {
    fn default() -> Self {
        Self {
            syntactic: 0.20,
            semantic: 0.25,
            normative: 0.30,
            temporal: 0.15,
            reflective: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContestResult {
    pub coherence_meter: f32,
    pub icr: f32, // Inconsistency Correction Ratio
    pub stability_field: f32,
    pub status: CoherenceStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CoherenceStatus {
    Unstable,    // 0-60%
    Adaptive,    // 61-80%
    Stable,      // 81-90%
    MetaCoherent,// 91-100%
}

impl ContestV2 {
    pub fn new() -> Self {
        Self {
            weights: MetricsWeights::default(),
        }
    }

    /// Calculates the Coherence Meter based on Whitepaper Formula:
    /// COHERENCE_METER = 1/4(ICR + NORM_SYNC + STABILITY_FIELD + (100 - ERR_DENSITY))
    pub fn calculate_coherence(
        &self, 
        icr: f32, 
        norm_sync: f32, 
        stability_field: f32, 
        err_density: f32
    ) -> ContestResult {
        
        let coherence = 0.25 * (icr + norm_sync + stability_field + (100.0 - err_density));
        
        let status = if coherence >= 91.0 {
            CoherenceStatus::MetaCoherent
        } else if coherence >= 81.0 {
            CoherenceStatus::Stable
        } else if coherence >= 61.0 {
            CoherenceStatus::Adaptive
        } else {
            CoherenceStatus::Unstable
        };

        ContestResult {
            coherence_meter: coherence,
            icr,
            stability_field,
            status,
        }
    }
}
