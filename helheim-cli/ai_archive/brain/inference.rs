use anyhow::{Result, anyhow};
use llama_cpp_2::model::{LlamaModel, AddBos, Special};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::token::data::LlamaTokenData;
use std::path::Path;
use std::num::NonZeroU32;
use std::io::Write;

/// The Thinking Engine
/// Generates text from the loaded GGUF model.
pub struct BrainInference;

impl BrainInference {
    pub fn infer(path_str: &str, prompt: &str) -> Result<String> {
        let path = Path::new(path_str);
        if !path.exists() {
            return Err(anyhow!("Brain file not found: {}", path_str));
        }

        // 1. Init Backend
        let backend = LlamaBackend::init()?;

        // 2. Load Model
        let model_params = llama_cpp_2::model::params::LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, path, &model_params)
            .map_err(|e| anyhow!("Failed to load model: {}", e))?;

        // 3. Create Context
        let n_ctx = NonZeroU32::new(2048).unwrap();
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(Some(n_ctx));
            
        let mut ctx = model.new_context(&backend, ctx_params)
            .map_err(|e| anyhow!("Failed to create context: {}", e))?;

        // 4. Tokenize Prompt (Dolphin-2.9-Llama-3 expects ChatML)
        let formatted_prompt = format!(
            "<|im_start|>system\nYou are PEPAI. Answer concisely.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
            prompt
        );

        // Dolphin/Llama3 usually needs BOS to start the sequence cleanly
        let tokens_list = model.str_to_token(&formatted_prompt, AddBos::Always)
            .map_err(|e| anyhow!("Tokenization failed: {}", e))?;
        
        // 5. Create Batch
        let mut batch = LlamaBatch::new(2048, 1);
        let last_index = tokens_list.len() as i32 - 1;

        for (i, token) in tokens_list.iter().enumerate() {
            // Only output logits for the last token of the prompt to generate next char
            let is_last = i as i32 == last_index;
            batch.add(*token, i as i32, &[0], is_last)?;
        }

        // 6. Decode Prompt
        ctx.decode(&mut batch)
            .map_err(|e| anyhow!("Decode failed: {}", e))?;

        // 7. Generate Response (Top-K Sampling)
        let mut response = String::new();
        let mut n_cur = tokens_list.len();

        while n_cur < 2048 && response.len() < 500 {
            // Get candidate tokens
            let candidates = ctx.candidates_ith(batch.n_tokens() - 1);
            
            // Sort by probability (descending)
            let mut sorted_candidates: Vec<_> = candidates.into_iter().collect();
            sorted_candidates.sort_by(|a, b| b.p().partial_cmp(&a.p()).unwrap_or(std::cmp::Ordering::Equal));

            // Top-K Sampling (K=5)
            // Filter Logic
            let mut clean_candidates = Vec::new();
            for candidate in sorted_candidates.iter() {
                 let text = model.token_to_str(candidate.id(), Special::Tokenize).unwrap_or("???".to_string());
                 // Filter Logic
                 if text.contains("<|im_start|>") || text.contains("<|start_header_id|>") || text.contains("!") || text.contains("#") {
                      continue; 
                 }
                 clean_candidates.push(*candidate);
                 if clean_candidates.len() >= 5 { break; }
            }
            
            // FALLBACK
            if clean_candidates.is_empty() {
                clean_candidates.push(sorted_candidates[0]);
            }
            
            // Weighted Random Pick
            let total_p: f32 = clean_candidates.iter().map(|c| c.p()).sum();
            let mut rng = rand::rng(); 
            let mut random_val = if total_p > 0.0 {
                 rand::Rng::random_range(&mut rng, 0.0..total_p)
            } else {
                 0.0
            };
            
            let mut best_token = clean_candidates[0];
            for candidate in clean_candidates.iter() {
                random_val -= candidate.p();
                if random_val <= 0.0 {
                    best_token = *candidate;
                    break;
                }
            }

            let token_id = best_token.id();
            let token_str = model.token_to_str(token_id, Special::Tokenize).unwrap_or("???".to_string());
            
            // DEBUG: Print EEG
            println!("EEG: [{}] -> '{}'", token_id, token_str);

            // Check for EOS
            if token_id == model.token_eos() || token_str.contains("<|im_end|>") || token_str.contains("<|eot_id|>") {
                println!("\n[BRAIN]: EOS Detected.");
                break;
            }

            response.push_str(&token_str);
            // print!("{}", token_str); // Disable raw stream during debug
            std::io::stdout().flush()?;

            // Prepare next batch
            batch.clear();
            batch.add(token_id, n_cur as i32, &[0], true)?;

            // Decode next
            ctx.decode(&mut batch)?;
            n_cur += 1;
        }

        Ok(response)
    }
}
