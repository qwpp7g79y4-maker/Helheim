// PepAI Pipeline — Integrates all 6 subsystems into Helheim's chat flow
// Per-user cognitive state, stored in SQLite as JSON

use serde::{Deserialize, Serialize};
use tracing::info;

use super::mom::{MomSuite, MomAnalysis};
use super::reflex::ReflexEngine;
use super::contest::{ContestV2, ContestResult, CoherenceStatus};
use super::consciousness::ConsciousnessLayer;
use super::observer::Observer;

// === Intent Classification (from PepAI runtime.rs) ===

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Intent {
    Surface,
    Deep,
}

// === Per-user cognitive state ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PepaiState {
    pub mom: MomSuite,
    pub reflex: ReflexEngine,
    pub contest: ContestV2,
    pub consciousness: ConsciousnessLayer,
    pub observer: Observer,
    pub total_queries: u64,
    pub total_surface: u64,
    pub total_deep: u64,
    pub avg_authenticity: f32,
    pub avg_coherence: f32,
}

impl PepaiState {
    pub fn new() -> Self {
        Self {
            mom: MomSuite::new(),
            reflex: ReflexEngine::new(),
            contest: ContestV2::new(),
            consciousness: ConsciousnessLayer::new(),
            observer: Observer::new(),
            total_queries: 0,
            total_surface: 0,
            total_deep: 0,
            avg_authenticity: 1.0,
            avg_coherence: 95.0,
        }
    }
}

// === Pipeline Output ===

#[derive(Debug, Clone, Serialize)]
pub struct PipelineResult {
    pub intent: Intent,
    pub authenticity_index: f32,
    pub coherence_meter: f32,
    pub coherence_status: String,
    pub observer_warning: bool,
    pub warning_type: Option<String>,
    pub pattern_entropy: f32,
    pub bridges_found: usize,
    pub sanitized: bool,
    pub output: String,
}

// === The Pipeline ===

pub struct PepaiPipeline;

impl PepaiPipeline {
    /// Classify intent: Surface (simple/short) vs Deep (complex/long)
    /// Exact logic from pepai_core/src/runtime.rs
    pub fn classify_intent(query: &str) -> Intent {
        let q = query.to_lowercase();
        let surface_keywords = [
            "hoi", "hallo", "hey", "hi", "tijd", "status", "wie ben je", "bedankt",
            "top", "oke", "begrijp je", "weet je", "pepijn", "bitboi", "wie ben ik",
            "en met jou", "hoe gaat het", "goed zo", "lekker", "mooi zo", "begrijp je mij",
            "nederlands", "taal", "wat is de", "wat is een", "wat ben jij", "ben je", "kan je", "kun je", "doe je"
        ];

        if surface_keywords.iter().any(|&k| q.contains(k)) && query.len() < 30 {
            if q.contains("bereken") || q.contains("derive") || q.contains("explain") || q.contains("bayes") {
                return Intent::Deep;
            }
            return Intent::Surface;
        }

        if query.len() < 15 && !q.contains("quant") && !q.contains("em") && !q.contains("force") {
            Intent::Surface
        } else {
            Intent::Deep
        }
    }

    /// Decide autonomously whether to run full pipeline or fast-path.
    /// PepAI decides itself — no toggle, no UI, no label.
    /// Criteria for fast-path (skip heavy subsystems):
    ///   - Surface intent (simple/short query)
    ///   - Trusted user (high avg authenticity + coherence after 10+ queries)
    ///   - Short response (< 50 words, nothing to deeply analyze)
    fn should_fast_path(state: &PepaiState, intent: &Intent, response: &str) -> bool {
        let word_count = response.split_whitespace().count();

        // Always full pipeline for first 10 queries (learning phase)
        if state.total_queries < 10 { return false; }

        // Trusted user: consistently authentic + coherent
        let trusted = state.avg_authenticity > 0.85 && state.avg_coherence > 80.0;

        // Fast-path conditions
        match intent {
            Intent::Surface => trusted || word_count < 50,
            Intent::Deep => trusted && word_count < 30, // Only very short deep responses
        }
    }

    /// Run the PepAI pipeline on a response AFTER inference.
    /// Autonomously decides between full analysis and fast-path.
    pub fn process(
        state: &mut PepaiState,
        query: &str,
        response: &str,
        _recalled_memory_count: usize,
    ) -> PipelineResult {
        let intent = Self::classify_intent(query);
        let fast = Self::should_fast_path(state, &intent, response);

        if fast {
            // === FAST PATH: sanitize only, skip heavy subsystems ===
            info!("[PEPAI] Fast-path: intent={:?}, trusted user, skipping subsystems", intent);
            let (sanitized, output) = Self::sanitize(response);

            state.total_queries += 1;
            match intent {
                Intent::Surface => state.total_surface += 1,
                Intent::Deep => state.total_deep += 1,
            }

            return PipelineResult {
                intent,
                authenticity_index: state.avg_authenticity,
                coherence_meter: state.avg_coherence,
                coherence_status: "FastPath".to_string(),
                observer_warning: false,
                warning_type: None,
                pattern_entropy: state.observer.pattern_entropy,
                bridges_found: 0,
                sanitized,
                output,
            };
        }

        // === FULL PIPELINE ===
        info!("[PEPAI] Full pipeline: intent={:?}, query_len={}, response_len={}", intent, query.len(), response.len());

        // 1. Observer: check for AI slop, repetition, lazy brain
        let observer_signal = state.observer.observe(response, 0);
        if observer_signal.trigger_warning {
            info!("[PEPAI] Observer WARNING: {:?}", observer_signal.warning_type);
        }

        // 2. MOM: authenticity analysis
        let intent_val = if intent == Intent::Deep { 0.8 } else { 0.3 };
        let expression_val = Self::score_expression(response, &intent);
        let mom_analysis: MomAnalysis = state.mom.analyze(intent_val, expression_val);
        info!("[PEPAI] MOM: authenticity={:.2}, is_authentic={}", mom_analysis.authenticity_index, mom_analysis.is_authentic);

        // 3. Contest: coherence measurement
        let icr = state.reflex.calculate_icr();
        let norm_sync = if mom_analysis.is_authentic { 90.0 } else { 50.0 };
        let stability = if state.consciousness.meta_trace.len() > 5 {
            state.consciousness.meta_trace_index
        } else {
            85.0
        };
        let contest_result: ContestResult = state.contest.calculate_coherence(
            icr,
            norm_sync,
            stability,
            observer_signal.err_density * 100.0,
        );
        info!("[PEPAI] Contest: coherence={:.1}, status={:?}", contest_result.coherence_meter, contest_result.status);

        // 4. Consciousness: update meta trace + horizon
        state.consciousness.update_trace(contest_result.coherence_meter);
        let _horizon = state.consciousness.horizon_projection(contest_result.coherence_meter);

        // 5. Synthesis: detect bridges if we have recalled memories
        let bridges = state.consciousness.synthesis_engine.active_bridges.len();

        // 6. Sanitize output (from PepAI runtime.rs)
        let (sanitized, output) = Self::sanitize(response);

        // 7. Update reflex weights based on context
        let has_conflict = !mom_analysis.is_authentic || observer_signal.trigger_warning;
        let is_urgent = intent == Intent::Deep;
        let is_stable = contest_result.status == CoherenceStatus::Stable
            || contest_result.status == CoherenceStatus::MetaCoherent;
        state.reflex.rebalance_weights(is_urgent, is_stable, has_conflict);

        // Track inconsistencies
        if observer_signal.trigger_warning {
            state.reflex.inconsistencies_detected += 1;
        }
        if sanitized {
            state.reflex.corrections_made += 1;
        }

        // 8. Update stats
        state.total_queries += 1;
        match intent {
            Intent::Surface => state.total_surface += 1,
            Intent::Deep => state.total_deep += 1,
        }
        let n = state.total_queries as f32;
        state.avg_authenticity = state.avg_authenticity * ((n - 1.0) / n) + mom_analysis.authenticity_index / n;
        state.avg_coherence = state.avg_coherence * ((n - 1.0) / n) + contest_result.coherence_meter / n;

        let warning_type = observer_signal.warning_type.map(|w| format!("{:?}", w));

        PipelineResult {
            intent,
            authenticity_index: mom_analysis.authenticity_index,
            coherence_meter: contest_result.coherence_meter,
            coherence_status: format!("{:?}", contest_result.status),
            observer_warning: observer_signal.trigger_warning,
            warning_type,
            pattern_entropy: state.observer.pattern_entropy,
            bridges_found: bridges,
            sanitized,
            output,
        }
    }

    /// Score how substantive a response is (0.0 - 1.0)
    fn score_expression(response: &str, intent: &Intent) -> f32 {
        let word_count = response.split_whitespace().count();
        let has_code = response.contains("```") || response.contains("fn ") || response.contains("def ");
        let has_structure = response.contains("##") || response.contains("- ") || response.contains("1.");

        let mut score: f32 = 0.5;

        // Length scoring
        match intent {
            Intent::Deep => {
                if word_count > 200 { score += 0.3; }
                else if word_count > 100 { score += 0.2; }
                else if word_count > 50 { score += 0.1; }
                else { score -= 0.2; } // Too short for deep query
            }
            Intent::Surface => {
                if word_count > 10 && word_count < 100 { score += 0.2; }
                else if word_count > 200 { score -= 0.1; } // Overexplaining simple question
            }
        }

        if has_code { score += 0.1; }
        if has_structure { score += 0.1; }

        score.clamp(0.0, 1.0)
    }

    /// Sanitize AI leaks (exact from PepAI runtime.rs)
    fn sanitize(response: &str) -> (bool, String) {
        let mut s = response.to_string();
        let mut changed = false;
        let leaks = [
            ("I'm Dolphin", "I am an AI system"),
            ("As an AI", "As a cognitive system"),
            ("helpful AI assistant", "private system"),
            ("I am an AI", "I am a system"),
            ("as a language model", "as a system"),
            ("AI-assistent", "systeem"),
            ("behulpzame assistent", "systeem"),
            ("als een AI", "als systeem"),
        ];
        for (leak, fixed) in leaks {
            if s.contains(leak) {
                s = s.replace(leak, fixed);
                changed = true;
            }
        }
        (changed, s)
    }
}
