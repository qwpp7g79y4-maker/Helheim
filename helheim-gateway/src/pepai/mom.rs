use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MomSuite {
    pub emo_map: EmoMap,
    pub intent_conduit: IntentConduit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmoMap {
    pub expected_emotion: String,
    pub detected_emotion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConduit {
    pub intent_vector: f32,
    pub expression_vector: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MomAnalysis {
    pub authenticity_index: f32,
    pub is_authentic: bool,
    pub delta_sense: f32,
}

impl MomSuite {
    pub fn new() -> Self {
        Self {
            emo_map: EmoMap { expected_emotion: "Neutral".into(), detected_emotion: "Neutral".into() },
            intent_conduit: IntentConduit { intent_vector: 1.0, expression_vector: 1.0 },
        }
    }

    /// Analyzes the authenticity of the interaction using the Whitepaper formula:
    /// Authenticity_Index = 1 - (|INT - EXP| / MAX(INT, EXP))
    pub fn analyze(&self, intent_val: f32, expression_val: f32) -> MomAnalysis {
        let max_val = f32::max(intent_val, expression_val).max(0.0001); // Avoid div by zero
        let delta = (intent_val - expression_val).abs();
        let auth_index = 1.0 - (delta / max_val);
        
        MomAnalysis {
            authenticity_index: auth_index,
            is_authentic: auth_index >= 0.75, // Threshold from Whitepaper
            delta_sense: delta,
        }
    }
}
