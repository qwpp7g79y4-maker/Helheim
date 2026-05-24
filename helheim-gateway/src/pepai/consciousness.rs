use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum IntentPosition {
    Analysis,   // Observation of context
    Reflection, // Moral consideration
    Correction, // Reformulation and re-weighting
}

use super::synthesis::SynthesisEngine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsciousnessLayer {
    pub int_pos: IntentPosition,
    pub meta_trace: Vec<f32>, // History of coherence scores
    pub meta_trace_index: f32, // Calculated cyclic consistency
    pub self_conflict_ratio: f32,
    pub synthesis_engine: SynthesisEngine,
}

impl ConsciousnessLayer {
    pub fn new() -> Self {
        Self {
            int_pos: IntentPosition::Analysis,
            meta_trace: Vec::new(),
            meta_trace_index: 0.0,
            self_conflict_ratio: 0.0,
            synthesis_engine: SynthesisEngine::new(),
        }
    }

    /// Updates the Meta Trace with the latest Coherence Score.
    /// Calculates new Meta Trace Index = Average correlation (simplified here as moving average).
    pub fn update_trace(&mut self, coherence_score: f32) {
        self.meta_trace.push(coherence_score);
        if self.meta_trace.len() > 100 {
            self.meta_trace.remove(0); // Keep last 100
        }
        
        let sum: f32 = self.meta_trace.iter().sum();
        self.meta_trace_index = sum / self.meta_trace.len() as f32;
    }

    /// Projects future stability (Horizon Projection).
    /// HORIZON_INDEX = COHERENCE(t) + Delta(t) * RateOfChange
    pub fn horizon_projection(&self, current_coherence: f32) -> f32 {
        if self.meta_trace.len() < 2 { return current_coherence; }
        
        let last = self.meta_trace[self.meta_trace.len() - 1];
        let prev = self.meta_trace[self.meta_trace.len() - 2];
        let rate_of_change = last - prev;
        
        current_coherence + (rate_of_change * 5.0) // Project 5 steps ahead
    }

    pub fn set_intent_position(&mut self, pos: IntentPosition) {
        self.int_pos = pos;
    }

    /// Checks alignment against Core Truths.
    /// Returns an alignment score (0.0 - 1.0).
    /// If alignment < 0.5, self_conflict_ratio increases.
    pub fn check_alignment(&mut self, response_vector: &[f32], truth_vectors: &[Vec<f32>]) -> f32 {
        let mut max_conflict: f32 = 0.0;

        for truth in truth_vectors {
            // Simplified Cosine Similarity for conflict detection
            // In reality, we need semantic logic here, but vector distance is a good proxy.
            // If vectors are too far apart (or inverse), it's a conflict.
            let similarity = self.cosine_similarity(response_vector, truth);
            
            // If response negates truth (similarity close to -1), conflict is high.
            if similarity < -0.5 {
                max_conflict = max_conflict.max(similarity.abs());
            }
        }

        // Update internal state
        if max_conflict > 0.5 {
            self.self_conflict_ratio += 0.1; // Conflict grows
        } else {
            self.self_conflict_ratio = (self.self_conflict_ratio - 0.05).max(0.0); // Conflict heals
        }

        1.0 - max_conflict
    }

    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        
        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return 0.0;
        }
        
        dot_product / (magnitude_a * magnitude_b)
    }
}
