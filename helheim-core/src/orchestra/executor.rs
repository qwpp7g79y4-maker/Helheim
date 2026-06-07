use colored::Colorize;
use anyhow::Result;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::orchestra::synthesis;
use helheim_lang::ast::CodeTaal;
use crate::orchestra::memory::{MemoryManager, HelheimType};
use crate::orchestra::system;

#[derive(Clone)]
pub struct Executor {
    pub memory: Arc<MemoryManager>,
    pub discovery: Arc<crate::network::DiscoveryService>,
}

impl Executor {
    pub fn new(memory: Arc<MemoryManager>, discovery: Arc<crate::network::DiscoveryService>) -> Self {
        Self { memory, discovery }
    }

    pub fn execute_ast(
        &self,
        ast: Vec<CodeTaal>,
        ctx: crate::common::context::ExecutionContext,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>>> + Send + '_>> {
        Box::pin(async move {
            for stmt in ast {
                if let Err(e) = ctx.check_timeout() {
                    return Err(e);
                }
                match stmt {
                    CodeTaal::GpuKernel(kernel_def) => {
                        println!("[EXECUTOR]: GpuKernel detectie: {}", kernel_def.name);
                        let backend = crate::gpu::get_backend();
                        match backend.compile(&kernel_def) {
                            Ok(compiled) => {
                                println!("[EXECUTOR]: Kernel succesvol gecompileerd op {}", backend.name());
                                // We zouden hier argumenten moeten resolven naar GpuPtr
                                if let Err(e) = backend.launch(&compiled, &[]) {
                                    eprintln!("[EXECUTOR ERROR]: Launch gefaald: {}", e);
                                } else {
                                    println!("[EXECUTOR]: Kernel gelanceerd!");
                                    let _ = backend.synchronize();
                                }
                            }
                            Err(e) => {
                                eprintln!("[EXECUTOR ERROR]: Compilatie gefaald: {}", e);
                            }
                        }
                    }
                    CodeTaal::MatMul { m, n, k } => {
                        println!(
                            "[KERNEL]: Synthesis of Tiled MatMul {}x{}x{} (Shared Memory Enabled)...",
                            m, n, k
                        );
                        // 1. Synthesize PTX (JIT)
                        let ptx = synthesis::KernelSynthesisEngine::synthesize(CodeTaal::MatMul {
                            m,
                            n,
                            k,
                        })
                        .unwrap_or_else(|_| String::new());

                        // 2. Execute on Hardware
                        println!("[GPU]: Launching Kernel on Nvidia RTX 5060 Ti...");
                        let id_a = crate::gpu::gpu_alloc_tensor_random(m, k).unwrap_or(0);
                        let id_b = crate::gpu::gpu_alloc_tensor_random(k, n).unwrap_or(0);
                        let id_c = crate::gpu::gpu_alloc_tensor_empty(m, n).unwrap_or(0);
                        match crate::gpu::gpu_execute_raw_ptx_ids(&ptx, id_a, id_b, id_c, m, n, k) {
                            Ok(gflops) => println!(
                                "[GPU]: ✅ Execution Complete. Performance: {:.2} GFLOPS",
                                gflops
                            ),
                            Err(e) => println!("[ERROR]: GPU Runtime Fail: {}", e),
                        }
                    }

                    CodeTaal::Return { value } => {
                        let eval = match value {
                            Some(box_val) => self.evaluate_ast_expr(&*box_val, ctx.clone()).await.unwrap_or_default(),
                            None => "".to_string(),
                        };
                        return Ok(Some(eval));
                    }
                    CodeTaal::Throw { message } => {
                        let eval = self.evaluate_expression(&message);
                        return Err(anyhow::anyhow!("Uncaught exception: {}", eval));
                    }
                    CodeTaal::RuneOp { command } => {
                        println!("[RUNE]: Executing bare-metal Rune...");
                        match unsafe { crate::common::rune::RuneEngine::execute_raw_rune(&command) } {
                            Ok(res) => println!("[RUNE_OUT]: {}", res),
                            Err(e) => println!("[RUNE_ERR]: {}", e),
                        }
                    }
                    CodeTaal::Print { message } => {
                        let evaluated_value = self.evaluate_expression(&message);
                        let resolved_val = self.memory.resolve_value(&evaluated_value);
                        // Strip quotes for printing strings cleanly
                        let clean_val = resolved_val.trim_matches('"');
                        println!("{}", clean_val);
                    }
                    CodeTaal::FileOp { action, path, content } => {
                        // Perform the I/O (beveiligde std bib)
                        // Resolve exprs to strings where possible (sync for paths)
                        let path_str = self.code_taal_to_string_sync(&path);
                        match action.as_str() {
                            "read" => {
                                match tokio::fs::read_to_string(&path_str).await {
                                    Ok(data) => {
                                        println!("[FS READ]: {} ({} bytes)", path_str, data.len());
                                        self.memory.set_var_native("__last_read".to_string(), crate::orchestra::memory::HelheimType::String(data.clone()));
                                    }
                                    Err(e) => println!("[FS READ ERROR]: {} : {}", path_str, e),
                                }
                            }
                            "write" => {
                                let content_str = if let Some(c) = content {
                                    self.code_taal_to_string_sync(&c)
                                } else { String::new() };
                                match tokio::fs::write(&path_str, content_str.as_bytes()).await {
                                    Ok(_) => println!("[FS WRITE]: {} ({} bytes)", path_str, content_str.len()),
                                    Err(e) => println!("[FS WRITE ERROR]: {} : {}", path_str, e),
                                }
                            }
                            _ => println!("[FS]: unknown action {}", action),
                        }
                    }
                    CodeTaal::HttpOp { method, url } => {
                        let url_str = self.code_taal_to_string_sync(&url);
                        if method.to_uppercase() == "GET" {
                            match ureq::get(&url_str).call() {
                                Ok(mut resp) => {
                                    let body = resp.body_mut().read_to_string().unwrap_or_default();
                                    println!("[HTTP GET]: {} -> {} bytes", url_str, body.len());
                                    self.memory.set_var_native("__last_http".to_string(), crate::orchestra::memory::HelheimType::String(body));
                                }
                                Err(e) => println!("[HTTP ERROR]: {} : {}", url_str, e),
                            }
                        } else {
                            println!("[HTTP]: {} {} (only GET supported in this lowering)", method, url_str);
                        }
                    }
                    CodeTaal::FunctionCall { name, args } => {
                        let mut resolved_args = Vec::new();
                        for a in args {
                            resolved_args.push(self.evaluate_ast_expr(&a, ctx.clone()).await.unwrap_or_default());
                        }
                        let _ = self.execute_function_call(&name, resolved_args, ctx.clone()).await?;
                    }
                    CodeTaal::Gebruik { path } => {
                        println!("[AST]: Laden van module: '{}'", path);
                        match tokio::fs::read_to_string(&path).await {
                            Ok(content) => {
                                match crate::orchestra::parser::HelParser::parse(&content) {
                                    Ok(module_ast) => {
                                        if let Err(e) = Box::pin(self.execute_ast(module_ast, ctx.clone())).await
                                        {
                                            println!("[ERROR]: Fout in module '{}': {}", path, e);
                                        }
                                    }
                                    Err(e) => println!(
                                        "[ERROR]: Kan module '{}' niet parsen: {}",
                                        path, e
                                    ),
                                }
                            }
                            Err(e) => println!("[ERROR]: Module '{}' niet gevonden: {}", path, e),
                        }
                    }
                    CodeTaal::FunctionDef { name, params, body } => {
                        self.memory.ast_funcs.insert(name.clone(), (params.clone(), body.clone()));
                        println!(
                            "[MEMORY]: Opslaan AST-functie '{}' met {} argumenten...",
                            name,
                            params.len()
                        );
                    }
                    CodeTaal::ModelDef { name, fields } => {
                        self.memory.model_store.insert(name.clone(), fields.clone());
                        println!("[MEMORY]: Blauwdruk opgeslagen voor model '{}' met {} velden.", name, fields.len());
                    }
                    CodeTaal::ModelInit { model_name, args: _args } => {
                        // Not used in execute_ast natively because VarDef intercepts 'nieuw'
                        println!("[AST]: Onverwachte losse ModelInit voor {}", model_name);
                    }
                    CodeTaal::VarDef { name, value } => {
                        // Extract literal or variable get, or resolve basic op
                        let value_str = match *value {
                            CodeTaal::Literal(ref l) => {
                                match l {
                                    helheim_lang::ast::LiteralValue::String(s) => format!("\"{}\"", s),
                                    _ => l.to_string(),
                                }
                            },
                            CodeTaal::VarGet { ref name } => self.memory.resolve_value(name),
                            CodeTaal::Op { .. } => {
                                let free_vars = helheim_lang::synthesis::collect_free_variables(&*value);
                                let mut context: std::collections::HashMap<String, helheim_lang::ast::LiteralValue> = std::collections::HashMap::new();
                                for name in free_vars {
                                    if let Some(typed) = self.memory.get_var_native(&name) {
                                        match typed {
                                            HelheimType::Bool(b) => {
                                                context.insert(name, helheim_lang::ast::LiteralValue::Int(if b { 1 } else { 0 }));
                                            }
                                            HelheimType::List(items) => {
                                                let mut mask: u32 = 0;
                                                for (i, item) in items.iter().take(32).enumerate() {
                                                    let is_true = match item {
                                                        serde_json::Value::Bool(b) => *b,
                                                        serde_json::Value::String(s) => s == "waar" || s == "true" || s == "1",
                                                        _ => false,
                                                    };
                                                    if is_true {
                                                        mask |= 1 << i;
                                                    }
                                                }
                                                context.insert(name, helheim_lang::ast::LiteralValue::Int(mask as i64));
                                            }
                                            HelheimType::Int(i) => {
                                                context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                                            }
                                            HelheimType::Float(f) => {
                                                context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                                            }
                                            _ => {
                                                let s = typed.to_string();
                                                context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                                            }
                                        }
                                    } else {
                                        let s = self.memory.resolve_value(&name);
                                        if let Ok(i) = s.parse::<i64>() {
                                            context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                                        } else if let Ok(f) = s.parse::<f64>() {
                                            context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                                        } else {
                                            context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                                        }
                                    }
                                }

                                let gpu_backend = crate::gpu::get_backend();
                                match gpu_backend.execute_lowered_block(&*value, &context) {
                                    Ok(Some(val)) => {
                                        println!("[EXECUTOR]: Op evaluated on GPU via PTX JIT path. Result: {}", val);
                                        let mask = val.to_bits() as u32;
                                        let mut spike_list = vec![];
                                        for i in 0..32 {
                                            let b = (mask & (1u32 << i)) != 0;
                                            spike_list.push(if b { "waar" } else { "onwaar" });
                                        }
                                        let unpacked = format!("[{}]", spike_list.join(", "));
                                        self.memory.set_var_native(name.clone(), HelheimType::parse(&unpacked));
                                        unpacked
                                    }
                                    _ => {
                                        // Let evaluate_ast_expr handle it cleanly so that nested Ops and logic work
                                        self.evaluate_ast_expr(&*value, ctx.clone()).await.unwrap_or_default()
                                    }
                                }
                            }
                            CodeTaal::FileOp { .. } | CodeTaal::HttpOp { .. } => {
                                // Perform I/O at VarDef time so `zet x = lees p` or `zet x = haal u` works
                                // We can't easily await here in the match without restructuring, so delegate to the top level handler
                                // by executing the sub expr (side effect + last read)
                                // For now fall back to the generic execution path for the value (it will run the I/O arm)
                                // and use the magic last read var.
                                let _ = Box::pin(self.execute_ast(vec![(*value).clone()], ctx.clone())).await;
                                self.memory.resolve_value("__last_read")
                            }
                            CodeTaal::ListLiteral { ref items } => {
                                // Set list in memory for SNN spikes etc.
                                let mut string_items = Vec::new();
                                let json_items: Vec<serde_json::Value> = items.iter().map(|l| match l {
                                    helheim_lang::ast::LiteralValue::Bool(b) => {
                                        string_items.push(if *b { "waar" } else { "onwaar" }.to_string());
                                        serde_json::json!(if *b { "waar" } else { "onwaar" })
                                    },
                                    helheim_lang::ast::LiteralValue::Int(i) => {
                                        string_items.push(i.to_string());
                                        serde_json::json!(*i)
                                    },
                                    helheim_lang::ast::LiteralValue::Float(f) => {
                                        string_items.push(f.to_string());
                                        serde_json::json!(*f)
                                    },
                                    helheim_lang::ast::LiteralValue::String(s) => {
                                        string_items.push(format!("\"{}\"", s));
                                        serde_json::json!(s)
                                    },
                                    helheim_lang::ast::LiteralValue::List(sub) => {
                                        string_items.push("[list]".to_string());
                                        serde_json::json!(sub.iter().map(|x| x.to_string()).collect::<Vec<_>>())
                                    },
                                }).collect();
                                self.memory.set_var_native(name.clone(), HelheimType::List(json_items));
                                format!("[{}]", string_items.join(", "))
                            }
                            CodeTaal::MatrixLiteral { ref rows } => {
                                // 2D spike tensor support
                                let mut flat: Vec<serde_json::Value> = Vec::new();
                                let mut string_items = Vec::new();
                                for row in rows {
                                    for item in row {
                                        let v = match item {
                                            helheim_lang::ast::LiteralValue::Bool(b) => {
                                                string_items.push(if *b { "waar" } else { "onwaar" }.to_string());
                                                serde_json::json!(if *b { "waar" } else { "onwaar" })
                                            },
                                            _ => {
                                                string_items.push(item.to_string());
                                                serde_json::json!(item.to_string())
                                            },
                                        };
                                        flat.push(v);
                                    }
                                }
                                self.memory.set_var_native(name.clone(), HelheimType::List(flat));
                                format!("[{}]", string_items.join(", "))
                            }
                            CodeTaal::Block { .. } => {
                                // Context binding + Spike Packing (Host-to-Device for SNN)
                                // If a free var is a List of bools, pack on CPU into u32 bitmask and pass as Int.
                                let free_vars = helheim_lang::synthesis::collect_free_variables(&*value);
                                let mut context: std::collections::HashMap<String, helheim_lang::ast::LiteralValue> = std::collections::HashMap::new();
                                for name in free_vars {
                                    if let Some(typed) = self.memory.get_var_native(&name) {
                                        match typed {
                                            HelheimType::Bool(b) => {
                                                context.insert(name, helheim_lang::ast::LiteralValue::Int(if b { 1 } else { 0 }));
                                            }
                                            HelheimType::List(items) => {
                                                // Pack boolean list into u32 bitmask for .b32 / bitwise
                                                let mut mask: u32 = 0;
                                                for (i, item) in items.iter().take(32).enumerate() {
                                                    let is_true = match item {
                                                        serde_json::Value::Bool(b) => *b,
                                                        serde_json::Value::String(s) => s == "waar" || s == "true" || s == "1",
                                                        _ => false,
                                                    };
                                                    if is_true {
                                                        mask |= 1 << i;
                                                    }
                                                }
                                                context.insert(name, helheim_lang::ast::LiteralValue::Int(mask as i64));
                                            }
                                            // 2D matrix of spikes: pack rows into multiple masks if needed (simple for small 2D)
                                            // For demo, pack first row or flatten bits
                                            // (full 2D support would allocate device tensor buffer)
                                            HelheimType::Int(i) => {
                                                context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                                            }
                                            HelheimType::Float(f) => {
                                                context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                                            }
                                            _ => {
                                                let s = typed.to_string();
                                                context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                                            }
                                        }
                                    } else {
                                        let s = self.memory.resolve_value(&name);
                                        if let Ok(i) = s.parse::<i64>() {
                                            context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                                        } else if let Ok(f) = s.parse::<f64>() {
                                            context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                                        } else {
                                            context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                                        }
                                    }
                                }

                                let gpu_backend = crate::gpu::get_backend();
                                match gpu_backend.execute_lowered_block(&*value, &context) {
                                    Ok(Some(val)) => {
                                        println!("[EXECUTOR]: Expression Block evaluated on GPU via PTX JIT path. Result: {}", val);
                                        // SNN host unpacking: if lowered returned packed bits (via b32 store in f32 pool), unpack to list string
                                        // Adapted for 2D: larger mask support (up to 32 for demo 2D matrices flattened or per-row)
                                        let mask = val.to_bits() as u32;
                                        let mut spike_list = vec![];
                                        for i in 0..32 {  // support larger 1D or flattened 2D spike tensors
                                            let b = (mask & (1u32 << i)) != 0;
                                            spike_list.push(if b { "waar" } else { "onwaar" });
                                        }
                                        let unpacked = format!("[{}]", spike_list.join(", "));
                                        // set nicely as list in memory too
                                        self.memory.set_var_native(name.clone(), HelheimType::parse(&unpacked));
                                        unpacked
                                    }
                                    _ => {
                                        // Fallback to CPU interpreter
                                        if let Some(ret) = Box::pin(self.execute_ast(vec![(*value).clone()], ctx.clone())).await.unwrap_or(None) {
                                            ret
                                        } else {
                                            "".to_string()
                                        }
                                    }
                                }
                            }
                            CodeTaal::FunctionCall { ref name, ref args } => {
                                let mut resolved_args = Vec::new();
                                for a in args {
                                    resolved_args.push(self.evaluate_ast_expr(a, ctx.clone()).await.unwrap_or_default());
                                }
                                self.execute_function_call(name, resolved_args, ctx.clone()).await.unwrap_or_default()
                            }
                            _ => "".to_string(),
                        };
                        
                        let mut evaluated_value = value_str.clone();
                        let clean_val = evaluated_value.trim();
                        if clean_val.starts_with("roep_aan ") || clean_val.starts_with("invoke ") {
                            let parts = crate::orchestra::parser::HelParser::tokenize(clean_val);
                            if parts.len() >= 2 {
                                let func_name = parts[1].value.clone();
                                let mut args = Vec::new();
                                if parts.len() > 2 {
                                    args = parts[2..].iter().map(|t| t.value.clone()).collect();
                                }
                                evaluated_value =
                                    self.execute_function_call(&func_name, args, ctx.clone()).await?;
                            }
                        } else if let Some(path) = clean_val.strip_prefix("gebruik ") {
                            let path = path.trim().trim_matches(';');
                            let resolved_path = self.memory.resolve_value(path);
                            if let Ok(content) = tokio::fs::read_to_string(&resolved_path).await {
                                if let Ok(module_ast) = crate::orchestra::parser::HelParser::parse(&content) {
                                    if let Some(ret_val) = Box::pin(self.execute_ast(module_ast, ctx.clone())).await.unwrap_or(None) {
                                         evaluated_value = ret_val;
                                    }
                                }
                            }
                        } else if let Some(prompt) = clean_val.strip_prefix("vraag ") {
                            let prompt = prompt.trim().trim_matches('"');
                            let resolved_prompt = self.memory.resolve_value(prompt);
                            use std::io::Write;
                            print!("{} ", resolved_prompt);
                            std::io::stdout().flush().unwrap_or(());
                            let mut input = String::new();
                            std::io::stdin().read_line(&mut input).unwrap_or(0);
                            evaluated_value = input.trim().to_string();
                        } else if let Some(path) = clean_val.strip_prefix("lees ") {
                            let path = path.trim().trim_matches('"');
                            let resolved_path = self.memory.resolve_value(path);
                            match std::fs::read_to_string(&resolved_path) {
                                Ok(content) => evaluated_value = content,
                                Err(e) => {
                                    println!("[ERROR]: Kan bestand '{}' niet lezen: {}", resolved_path, e);
                                    evaluated_value = "".to_string();
                                }
                            }
                        } else if let Some(model_init) = clean_val.strip_prefix("nieuw ") {
                            // Format: nieuw Server("192.168.1.1", 9000)
                            let mut parts = model_init.splitn(2, '(');
                            let model_name = parts.next().unwrap_or("").trim().to_string();
                            let args_str = parts.next().unwrap_or("").trim().trim_end_matches(')');
                            
                            let mut args = Vec::new();
                            for arg in args_str.split(',') {
                                let arg_val = arg.trim().trim_matches('"').to_string();
                                if !arg_val.is_empty() {
                                    args.push(self.memory.resolve_value(&arg_val));
                                }
                            }
                            
                            let fields_opt = self.memory.model_store.get(&model_name).map(|v| v.value().clone());
                            if let Some(fields) = fields_opt {
                                if fields.len() != args.len() {
                                    println!("[ERROR]: Model '{}' verwacht {} argumenten, kreeg er {}.", model_name, fields.len(), args.len());
                                    evaluated_value = "null".to_string();
                                } else {
                                    let mut json_map = serde_json::Map::new();
                                    for (i, field) in fields.iter().enumerate() {
                                        let val_str: &str = &args[i];
                                        let json_val = if let Ok(num) = val_str.parse::<f64>() {
                                            serde_json::json!(num)
                                        } else if val_str == "waar" || val_str == "true" {
                                            serde_json::json!(true)
                                        } else if val_str == "onwaar" || val_str == "false" {
                                            serde_json::json!(false)
                                        } else {
                                            serde_json::json!(val_str)
                                        };
                                        json_map.insert(field.clone(), json_val);
                                    }
                                    evaluated_value = serde_json::to_string(&json_map).unwrap_or_else(|_| "null".to_string());
                                }
                            } else {
                                println!("[ERROR]: Model '{}' is niet gedefinieerd.", model_name);
                                evaluated_value = "null".to_string();
                            }
                        } else {
                            evaluated_value = self.evaluate_expression(&value_str);
                        }
                        let evaluated_value = self.memory.resolve_value(&evaluated_value);
                        println!("[MEM]: {} = {}", name, evaluated_value);
                        self.memory.set_var_native(name, HelheimType::parse(&evaluated_value));
                    }
                    CodeTaal::VarGet { name } => {
                        if let Some(val) = self.memory.get_var_native(&name) {
                            println!("[VAL]: {} = {}", name, val);
                        } else {
                            println!("[ERR]: Variabele '{}' niet gevonden.", name);
                        }
                    }
                    CodeTaal::Loop { condition, body } => {
                        // Very simple infinite loop guard
                        let mut iterations = 0;
                        loop {
                            let should_run = self.evaluate_ast_condition(&condition, ctx.clone()).await;
                            if !should_run || iterations > 1000 {
                                break;
                            }

                            // Propagate return from body (zolang containing als/retourneer etc.)
                            if let Some(ret) = self.propagate_return(&body, ctx.clone()).await? {
                                return Ok(Some(ret));
                            }
                            iterations += 1;
                        }
                    }
                    CodeTaal::ForEach {
                        iterator,
                        iterable,
                        body,
                    } => {
                        let json_val = self.memory.resolve_value(&iterable);
                        let mut clone_statements = Vec::new();
                        if let CodeTaal::Block { statements } = *body.clone() {
                            clone_statements = statements;
                        }

                        // Try parsing JSON list
                        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&json_val) {
                            println!(
                                "[LOOP]: 'voor elke' geactiveerd met {} iteraties over '{}'.",
                                arr.len(),
                                iterable
                            );
                            for v in arr {
                                let item_str = if let Some(s) = v.as_str() {
                                    s.to_string()
                                } else {
                                    v.to_string()
                                };
                                self.memory.set_var_native(iterator.clone(), HelheimType::parse(&item_str));
                                // Use propagate helper for return from for-each body
                                let body_block = CodeTaal::Block { statements: clone_statements.clone() };
                                if let Some(ret) = self.propagate_return(&body_block, ctx.clone()).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        } else {
                            println!(
                                "[ERROR]: Kan '{}' niet itereren. Waarde is geen geldige JSON-lijst.",
                                iterable
                            );
                        }
                    }
                    CodeTaal::If {
                        condition,
                        then,
                        else_block,
                    } => {
                        if self.evaluate_ast_condition(&condition, ctx.clone()).await {
                            if let Some(ret) = self.propagate_return(&then, ctx.clone()).await? {
                                return Ok(Some(ret));
                            }
                        } else if let Some(else_b) = else_block {
                            if let Some(ret) = self.propagate_return(&else_b, ctx.clone()).await? {
                                return Ok(Some(ret));
                            }
                        }
                    }
                    CodeTaal::ArrayPush { array_name, value } => {
                        let args = vec![array_name.clone(), value.clone()];
                        let _ = self.execute_function_call("voeg_toe", args, ctx.clone()).await?;
                    }
                    CodeTaal::ArrayRemove { array_name, index } => {
                        let args = vec![array_name.clone(), index.clone()];
                        let _ = self.execute_function_call("verwijder", args, ctx.clone()).await?;
                    }
                    CodeTaal::Concurrent { statements } => {
                        println!(
                            "[AST]: Activeren van parallelle uitvoering ({} taken)...",
                            statements.len()
                        );
                        let mut futures_list = Vec::new();
                        for concurrent_stmt in statements {
                            // Execute each statement in its own context
                            futures_list.push(self.execute_ast(vec![concurrent_stmt.clone()], ctx.clone()));
                        }

                        // Await all of them simultaneously
                        let results = futures::future::join_all(futures_list).await;
                        for res in results {
                            if let Err(e) = res {
                                println!("[ERROR]: Fout in parallelle taak: {}", e);
                            }
                        }
                    }
                    CodeTaal::Block { statements: _ } => {
                        // Context binding + Spike Packing (Host-to-Device for SNN)
                        let free_vars = helheim_lang::synthesis::collect_free_variables(&stmt);
                        let mut context: std::collections::HashMap<String, helheim_lang::ast::LiteralValue> = std::collections::HashMap::new();
                        for name in free_vars {
                            if let Some(typed) = self.memory.get_var_native(&name) {
                                match typed {
                                    HelheimType::Bool(b) => {
                                        context.insert(name, helheim_lang::ast::LiteralValue::Int(if b { 1 } else { 0 }));
                                    }
                                    HelheimType::List(items) => {
                                        let mut mask: u32 = 0;
                                        for (i, item) in items.iter().take(32).enumerate() {
                                            let is_true = match item {
                                                serde_json::Value::Bool(b) => *b,
                                                serde_json::Value::String(s) => s == "waar" || s == "true" || s == "1",
                                                _ => false,
                                            };
                                            if is_true {
                                                mask |= 1 << i;
                                            }
                                        }
                                        context.insert(name, helheim_lang::ast::LiteralValue::Int(mask as i64));
                                    }
                                    HelheimType::Int(i) => {
                                        context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                                    }
                                    HelheimType::Float(f) => {
                                        context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                                    }
                                    _ => {
                                        let s = typed.to_string();
                                        context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                                    }
                                }
                            } else {
                                let s = self.memory.resolve_value(&name);
                                if let Ok(i) = s.parse::<i64>() {
                                    context.insert(name, helheim_lang::ast::LiteralValue::Int(i));
                                } else if let Ok(f) = s.parse::<f64>() {
                                    context.insert(name, helheim_lang::ast::LiteralValue::Float(f));
                                } else {
                                    context.insert(name, helheim_lang::ast::LiteralValue::String(s.trim_matches('"').to_string()));
                                }
                            }
                        }

                        let gpu_backend = crate::gpu::get_backend();
                        match gpu_backend.execute_lowered_block(&stmt, &context) {
                            Ok(Some(val)) => {
                                println!("[EXECUTOR]: Lowered block executed on real GPU via PTX JIT path. Return: {}", val);
                                // SNN unpacking for direct block return
                                // Adapted for 2D matrices: support larger flattened spike results (32 bits demo for  e.g. 4x8 or 8x4 2D spike tensors)
                                let mask = val.to_bits() as u32;
                                let mut spike_list = vec![];
                                for i in 0..32 {
                                    let b = (mask & (1u32 << i)) != 0;
                                    spike_list.push(if b { "waar" } else { "onwaar" });
                                }
                                let unpacked = format!("[{}]", spike_list.join(", "));
                                return Ok(Some(unpacked));
                            }
                            Ok(None) => {
                                println!("[EXECUTOR]: Lowered block executed on real GPU via PTX JIT path. No return value.");
                            }
                            Err(e) => {
                                println!("[EXECUTOR]: GPU lowered launch not taken ({}), falling back to interpreter", e);
                                if let CodeTaal::Block { statements } = &stmt {
                                    if let Some(ret) = Box::pin(self.execute_ast(statements.clone(), ctx.clone())).await? {
                                        return Ok(Some(ret));
                                    }
                                }
                            }
                        }
                    }
                    CodeTaal::Daemon { body } => {
                        println!("[AST]: Achtergrond (Daemon) proces gestart...");
                        let engine_clone = self.clone();
                        let body_clone = body.clone();
                        let ctx_clone = ctx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = engine_clone.execute_ast(vec![*body_clone], ctx_clone).await {
                                println!("[ERROR]: Fout in daemon proces: {}", e);
                            }
                        });
                    }
                    CodeTaal::TryCatch {
                        try_block,
                        catch_block,
                        error_var,
                    } => {
                        let statements = if let CodeTaal::Block { statements } = *try_block.clone()
                        {
                            statements
                        } else {
                            Vec::new()
                        };
                        match self.execute_ast(statements, ctx.clone()).await {
                            Ok(Some(ret)) => return Ok(Some(ret)),
                            Ok(None) => {}
                            Err(e) => {
                                println!("[VANG]: Fout afgevangen: {}", e);
                                if let Some(err_name) = error_var {
                                    self.memory.set_var_native(err_name.clone(), HelheimType::String(e.to_string()));
                                }
                                // Propagate return from catch block as well
                                if let Some(ret) = self.propagate_return(&catch_block, ctx.clone()).await? {
                                    return Ok(Some(ret));
                                }
                            }
                        }
                    }
                    CodeTaal::Send { target, payload } => {
                        let clean_payload = payload.trim().trim_matches('"');

                        // 1. String Interpolation (Basic: check for $vars)
                        let mut final_payload = clean_payload.to_string();
                        if final_payload.contains('$') {
                            let store = self.memory.local_stack.lock().unwrap_or_else(|e| e.into_inner());
                            for scope in store.iter().rev() {
                                for (k, v) in scope.iter() {
                                    let key = format!("${}", k);
                                    if final_payload.contains(&key) {
                                        let val_str = match v {
                                            HelheimType::String(s) => s.clone(),
                                            _ => v.to_string(),
                                        };
                                        final_payload = final_payload.replace(&key, &val_str);
                                    }
                                }
                            }
                            for entry in self.memory.globals.iter() {
                                let key = format!("${}", entry.key());
                                if final_payload.contains(&key) {
                                    let val_str = match entry.value() {
                                        HelheimType::String(s) => s.clone(),
                                        _ => entry.value().to_string(),
                                    };
                                    final_payload = final_payload.replace(&key, &val_str);
                                }
                            }
                        }

                        println!("[AST]: Sturen naar '{}': {}", target, final_payload);

                        // 2. Broadcast Logic
                        let mut final_targets = Vec::new();
                        if target == "allemaal" {
                            if let Ok(peers) = self.discovery.peers.lock() {
                                for ip in peers.keys() {
                                    final_targets.push(ip.clone());
                                }
                            }
                        } else {
                            final_targets.push(target.clone());
                        }

                        // 3. Dispatch
                        for t in final_targets {
                            let _ = crate::network::swarm::SwarmEngine::dispatch(
                                &t,
                                9003,
                                &final_payload,
                            )
                            .await;
                        }
                    }
                    CodeTaal::SysOp { command } => {
                        // We willen ELK WOORD afzonderlijk resolven, voor het geval 
                        // de gebruiker 'voer uit echo NAAM' typt zonder $ of {}
                        let mut resolved_parts = Vec::new();
                        for part in command.split_whitespace() {
                            let resolved = self.memory.resolve_value(part);
                            // Strip quotes to ensure clean bash arguments
                            let clean = resolved.trim_matches('"');
                            resolved_parts.push(clean.to_string());
                        }
                        let resolved_command = resolved_parts.join(" ");
                        
                        let mut args = vec![];
                        // If it starts with "voer uit ", strip it for native shell execution.
                        // Otherwise pass the whole command string to "systeem.shell"
                        if let Some(cmd) = resolved_command.strip_prefix("voer uit ") {
                            args.push(cmd.to_string());
                        } else {
                            args.push(resolved_command);
                        }
                        if let Some(output) = system::SystemManager::try_execute_native(&self.memory, "systeem.shell", &args, &ctx).await? {
                            if !output.is_empty() {
                                println!("{}", output);
                            }
                        }
                        // Recursively call process_command for legacy support
                        // Note: process_command is async, so we await it.
                        
                    }
                    _ => println!("[AST]: Instructie nog niet geïmplementeerd: {:?}", stmt),
                }
            }
            Ok(None)
        })
    }

    /// Compact helper for Return propagation within nested scopes (Optie 1 - Fase 1.2).
    /// Any Return (retourneer/geef_terug/return) deep inside als/zolang/try etc. makes
    /// execute_ast return Ok(Some(value)). This helper + early returns in control arms
    /// ensure the function call stack is aborted immediately and we return the value
    /// to the original caller, while the function wrapper guarantees pop_scope on every path.
    async fn propagate_return(&self, body: &CodeTaal, ctx: crate::common::context::ExecutionContext) -> Result<Option<String>> {
        match body {
            CodeTaal::Block { statements } => self.execute_ast(statements.clone(), ctx).await,
            other => self.execute_ast(vec![other.clone()], ctx).await,
        }
    }

    async fn execute_function_call(&self, name: &str, args: Vec<String>, ctx: crate::common::context::ExecutionContext) -> Result<String> {
        if name == "tekst" && args.len() == 1 {
            let inner_val = self.memory.resolve_value(&args[0]);
            return Ok(inner_val.trim_matches('"').to_string());
        }
        if name == "nummer" && args.len() == 1 {
            let inner_val = self.memory.resolve_value(&args[0]);
            if let Ok(num) = inner_val.parse::<f64>() {
                return Ok(num.to_string());
            } else {
                return Ok("0".to_string());
            }
        }

        // 1. Try Native System Library
        if let Some(res) = system::SystemManager::try_execute_native(&self.memory, name, &args, &ctx).await? {
            return Ok(res);
        }

        // 2. Try User-Defined AST Function (pure CodeTaal general path)
        let func_tuple = self.memory.ast_funcs.get(name).map(|v| v.value().clone());

        if let Some((params, body)) = func_tuple {
            let mut resolved_args = Vec::new();
            for i in 0..params.len() {
                if i < args.len() {
                    resolved_args.push(self.memory.resolve_value(&args[i]));
                } else {
                    resolved_args.push("".to_string());
                }
            }

            let _scope_guard = crate::orchestra::memory::ScopeGuard::new(&self.memory);

            for (i, param) in params.iter().enumerate() {
                self.memory.set_var_native(param.clone(), HelheimType::parse(&resolved_args[i]));
            }

            // Robust return propagation:
            // propagate_return + the early `return Ok(Some(ret))` in If/Loop/ForEach/TryCatch
            // make a deep `retourneer` from inside geneste als/zolang immediately unwind
            // all the way out of this function's execute_ast call.
            let result = match self.propagate_return(&body, ctx.clone()).await {
                Ok(Some(ret)) => ret,
                Ok(None) => "".to_string(),
                Err(e) => return Err(e),
            };

            Ok(result)
        } else {
            println!("[ERR]: Functie '{}' bestaat niet in AST store of Native Library.", name);
            Ok("".to_string())
        }
    }

    /// Helper to turn a CodeTaal expr (Literal/VarGet) into a usable String for I/O paths/urls/content.
    /// (I/O performing cases are handled at statement level to avoid async recursion.)
    fn code_taal_to_string_sync(&self, expr: &CodeTaal) -> String {
        match expr {
            CodeTaal::Literal(lit) => lit.to_string().trim_matches('"').to_string(),
            CodeTaal::VarGet { name } => self.memory.resolve_value(name),
            // For complex exprs that are themselves I/O, we just use a placeholder here; the statement arm will have executed it
            _ => "".to_string(),
        }
    }


    pub async fn evaluate_condition(&self, condition: &str) -> bool {
        if condition.starts_with("bestand_bestaat ") {
            let path = condition[16..].trim().trim_matches('"');
            return tokio::fs::try_exists(path).await.unwrap_or(false);
        }

        let result = self.evaluate_expression(condition);
        if result == "waar" {
            return true;
        }
        if result == "onwaar" {
            return false;
        }

        println!(
            "[LOGIC]: Onbekende of ongeldige conditie: '{}' (Geëvalueerd tot '{}')",
            condition, result
        );
        false
    }
    async fn evaluate_ast_condition(&self, cond: &CodeTaal, ctx: crate::common::context::ExecutionContext) -> bool {
        let evaluated = self.evaluate_ast_expr(cond, ctx).await.unwrap_or_default();
        self.evaluate_condition(&evaluated).await
    }

    pub fn evaluate_ast_expr<'a>(&'a self, expr: &'a CodeTaal, ctx: crate::common::context::ExecutionContext) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(async move {
            match expr {
                CodeTaal::Literal(l) => Ok(l.to_string().trim_matches('"').to_string()),
                CodeTaal::VarGet { name } => Ok(self.memory.resolve_value(name)),
                CodeTaal::Op { left, op, right } => {
                    let l = self.evaluate_ast_expr(left, ctx.clone()).await?;
                    let r = self.evaluate_ast_expr(right, ctx.clone()).await?;
                    
                    // --- SNN Intrinsics CPU Fallback ---
                    if op == "popc" {
                        let mut count = 0;
                        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&l) {
                            for val in arr {
                                if let Some(s) = val.as_str() {
                                    if s == "waar" || s == "true" || s == "1" { count += 1; }
                                } else if let Some(b) = val.as_bool() {
                                    if b { count += 1; }
                                } else if let Some(n) = val.as_i64() {
                                    if n == 1 { count += 1; }
                                }
                            }
                        } else {
                            count = l.matches("waar").count() + l.matches("true").count();
                        }
                        return Ok(count.to_string());
                    }

                    if op == "&" {
                        if l.starts_with('[') && r.starts_with('[') {
                            if let (Ok(arr_l), Ok(arr_r)) = (
                                serde_json::from_str::<Vec<serde_json::Value>>(&l),
                                serde_json::from_str::<Vec<serde_json::Value>>(&r)
                            ) {
                                let mut res = Vec::new();
                                for (vl, vr) in arr_l.iter().zip(arr_r.iter()) {
                                    let l_is_true = vl.as_str().map(|s| s == "waar" || s == "true").unwrap_or(false) || vl.as_bool().unwrap_or(false) || vl.as_i64().unwrap_or(0) == 1;
                                    let r_is_true = vr.as_str().map(|s| s == "waar" || s == "true").unwrap_or(false) || vr.as_bool().unwrap_or(false) || vr.as_i64().unwrap_or(0) == 1;
                                    res.push(if l_is_true && r_is_true { "waar" } else { "onwaar" });
                                }
                                return Ok(serde_json::to_string(&res).unwrap_or_default());
                            }
                        }
                    }
                    // -----------------------------------

                    let to_evalexpr_literal = |val: &str| -> String {
                        if val == "waar" || val == "true" { return "true".to_string(); }
                        if val == "onwaar" || val == "false" { return "false".to_string(); }
                        if val.parse::<f64>().is_ok() { return val.to_string(); }
                        format!("\"{}\"", val.replace("\"", "\\\""))
                    };
                    
                    let l_lit = to_evalexpr_literal(&l);
                    let r_lit = to_evalexpr_literal(&r);
                    let expr_str = format!("{} {} {}", l_lit, op, r_lit);
                    Ok(self.evaluate_expression(&expr_str))
                }
                CodeTaal::FunctionCall { name, args } => {
                    let mut resolved_args = Vec::new();
                    for a in args {
                        resolved_args.push(self.evaluate_ast_expr(a, ctx.clone()).await.unwrap_or_default());
                    }
                    self.execute_function_call(name, resolved_args, ctx).await
                }
                _ => Ok("".to_string()),
            }
        })
    }

    fn evaluate_expression(&self, expr: &str) -> String {
        let expr_clean = expr.trim();

        // Native STD LIB: lengte(Lijst)
        if expr_clean.starts_with("lengte(") && expr_clean.ends_with(")") {
            let inner = expr_clean[7..expr_clean.len() - 1].trim();
            let inner_val = self.memory.resolve_value(inner);
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&inner_val) {
                return arr.len().to_string();
            } else {
                return inner_val.len().to_string();
            }
        }

        // Tensor Allocation Intercept (Phase 6)
        if expr_clean.starts_with("tensor(")
            && expr_clean.ends_with(")")
            && !expr_clean.contains("id=")
        {
            let dim: Vec<&str> = expr_clean[7..expr_clean.len() - 1].split(',').collect();
            if dim.len() == 2 {
                let m = dim[0].trim().parse::<usize>().unwrap_or(0);
                let n = dim[1].trim().parse::<usize>().unwrap_or(0);
                if m > 0 && n > 0 {
                    println!("[AST]: Nieuwe Tensor allocatie ({}x{})...", m, n);
                    match crate::gpu::gpu_alloc_tensor_random(m, n) {
                        Ok(id) => return format!("tensor({}, {}, id={})", m, n, id),
                        Err(e) => return format!("ERROR: VRAM Allocatie gefaald: {}", e),
                    }
                }
            }
        }

        // Tensor ReLU Intercept (Project Apex)
        if expr_clean.starts_with("relu(") && expr_clean.ends_with(")") {
            let inner = expr_clean[5..expr_clean.len() - 1].trim();
            let inner_val = self.memory.resolve_value(inner);
            if inner_val.starts_with("tensor(") && inner_val.contains("id=") {
                let parts: Vec<&str> = inner_val[7..inner_val.len() - 1].split(',').collect();
                if parts.len() == 3 {
                    let m = parts[0].trim().parse::<usize>().unwrap_or(0);
                    let n = parts[1].trim().parse::<usize>().unwrap_or(0);
                    let id_a = parts[2]
                        .trim()
                        .replace("id=", "")
                        .parse::<usize>()
                        .unwrap_or(0);
                    if m > 0 && n > 0 {
                        println!(
                            "[AST]: Tensor Activering (ReLU) gedetecteerd op {}x{}...",
                            m, n
                        );
                        let out_id = crate::gpu::gpu_alloc_tensor_empty(m, n).unwrap_or(0);
                        let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                            CodeTaal::TensorRelu { m, n },
                        )
                        .unwrap_or_else(|_| String::new());
                        match crate::gpu::gpu_execute_tensor_relu(&ptx, id_a, out_id, m, n) {
                            Ok(gflops) => println!(
                                "[GPU]: ✅ Tensor ReLU voltooid. Performance: {:.2} GFLOPS",
                                gflops
                            ),
                            Err(e) => println!("[ERROR]: GPU Tensor ReLU Fail: {}", e),
                        }
                        return format!("tensor({}, {}, id={})", m, n, out_id);
                    }
                }
            }
        }

        // --- TENSOR INTERCEPTS (Project Apex) ---
        // If the expression looks like a simple arithmetic operation, check if it's tensor math
        let parts: Vec<&str> = expr_clean.split_whitespace().collect();
        let mut left_val = String::new();
        let mut right_val = String::new();
        let mut op = "";
        if parts.len() == 3 {
            op = parts[1];
            left_val = self.memory.resolve_value(parts[0]);
            right_val = self.memory.resolve_value(parts[2]);
        }

        // Tensor Multiplication Intercept (Project Apex-WMMA)
        if left_val.starts_with("tensor(") && right_val.starts_with("tensor(") && op == "*" {
            let l_dim: Vec<&str> = left_val[7..left_val.len() - 1].split(',').collect();
            let r_dim: Vec<&str> = right_val[7..right_val.len() - 1].split(',').collect();
            if l_dim.len() == 3 && r_dim.len() == 3 {
                let m = l_dim[0].trim().parse::<usize>().unwrap_or(0);
                let k1 = l_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_a = l_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                let k2 = r_dim[0].trim().parse::<usize>().unwrap_or(0);
                let n = r_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_b = r_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                if k1 == k2 && k1 > 0 {
                    println!(
                        "[AST]: Tensor vermenigvuldiging gedetecteerd. Matrix {}x{} * {}x{}...",
                        m, k1, k2, n
                    );
                    let out_id = crate::gpu::gpu_alloc_tensor_empty(m, n).unwrap_or(0);
                    let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                        CodeTaal::MatMul { m, n, k: k1 },
                    )
                    .unwrap_or_else(|_| String::new());
                    println!("[GPU]: Activeren van WMMA Tensor Cores (Project Apex)...");
                    match crate::gpu::gpu_execute_raw_ptx_ids(&ptx, id_a, id_b, out_id, m, n, k1) {
                        Ok(gflops) => println!(
                            "[GPU]: ✅ Tensor Executie voltooid. Performance: {:.2} GFLOPS",
                            gflops
                        ),
                        Err(e) => {
                            println!("[GPU ERROR]: {} - Terugvallen op CPU (Rayon)...", e);
                            match crate::gpu::cpu_execute_matmul(id_a, id_b, out_id, m, n, k1) {
                                Ok(gflops) => println!(
                                    "[CPU]: ✅ Tensor Executie voltooid (Fallback). Performance: {:.2} GFLOPS",
                                    gflops
                                ),
                                Err(e) => println!("[CPU ERROR]: {}", e),
                            }
                        }
                    }
                    return format!("tensor({}, {}, id={})", m, n, out_id);
                } else {
                    println!(
                        "[ERROR]: Tensor dimensies komen niet overeen ({}x{} * {}x{})",
                        m, k1, k2, n
                    );
                }
            }
        }

        // Tensor Addition Intercept (Project Apex-WMMA)
        if left_val.starts_with("tensor(") && right_val.starts_with("tensor(") && op == "+" {
            let l_dim: Vec<&str> = left_val[7..left_val.len() - 1].split(',').collect();
            let r_dim: Vec<&str> = right_val[7..right_val.len() - 1].split(',').collect();
            if l_dim.len() == 3 && r_dim.len() == 3 {
                let m1 = l_dim[0].trim().parse::<usize>().unwrap_or(0);
                let n1 = l_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_a = l_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                let m2 = r_dim[0].trim().parse::<usize>().unwrap_or(0);
                let n2 = r_dim[1].trim().parse::<usize>().unwrap_or(0);
                let id_b = r_dim[2]
                    .trim()
                    .replace("id=", "")
                    .parse::<usize>()
                    .unwrap_or(0);

                if m1 == m2 && n1 == n2 && m1 > 0 {
                    println!(
                        "[AST]: Tensor Optelling gedetecteerd. Matrix {}x{} + {}x{}...",
                        m1, n1, m2, n2
                    );
                    let out_id = crate::gpu::gpu_alloc_tensor_empty(m1, n1).unwrap_or(0);
                    let ptx = crate::orchestra::synthesis::KernelSynthesisEngine::synthesize(
                        CodeTaal::TensorAdd { m: m1, n: n1 },
                    )
                    .unwrap_or_else(|_| String::new());
                    match crate::gpu::gpu_execute_tensor_add(&ptx, id_a, id_b, out_id, m1, n1) {
                        Ok(gflops) => println!(
                            "[GPU]: ✅ Tensor Optelling voltooid. Performance: {:.2} GFLOPS",
                            gflops
                        ),
                        Err(e) => {
                            println!("[GPU ERROR]: {} - Terugvallen op CPU (Rayon)...", e);
                            match crate::gpu::cpu_execute_tensor_add(id_a, id_b, out_id, m1, n1) {
                                Ok(gflops) => println!(
                                    "[CPU]: ✅ Tensor Optelling voltooid (Fallback). Performance: {:.2} GFLOPS",
                                    gflops
                                ),
                                Err(e) => println!("[CPU ERROR]: {}", e),
                            }
                        }
                    }
                    return format!("tensor({}, {}, id={})", m1, n1, out_id);
                }
            }
        }

        // --- PHASE 7: ROBUST EXPRESSION EVALUATOR (evalexpr) ---
        // If it's not a tensor operation, try to evaluate it as a complex math/logic expression
        if !expr_clean.starts_with("tensor(") && !expr_clean.contains("tensor(") {
            use evalexpr::ContextWithMutableVariables;
            use evalexpr::Context;
            let mut context: evalexpr::HashMapContext = evalexpr::HashMapContext::new();
            {
                let store = self.memory.local_stack.lock().unwrap_or_else(|e| e.into_inner());
                for scope in store.iter().rev() {
                    for (k, v) in scope.iter() {
                        if let HelheimType::Int(num_int) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Int(*num_int));
                        } else if let HelheimType::Float(num_float) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Float(*num_float));
                        } else if let HelheimType::Bool(b) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Boolean(*b));
                        } else {
                            let val_str = match v {
                                HelheimType::String(s) => s.clone(),
                                _ => v.to_string(),
                            };
                            let _ = context.set_value(k.clone(), val_str.into());
                        }
                    }
                }
                for entry in self.memory.globals.iter() {
                    let k = entry.key();
                    let v = entry.value();
                    if context.get_value(k).is_none() {
                        if let HelheimType::Int(num_int) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Int(*num_int));
                        } else if let HelheimType::Float(num_float) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Float(*num_float));
                        } else if let HelheimType::Bool(b) = v {
                            let _ = context.set_value(k.clone(), evalexpr::Value::Boolean(*b));
                        } else {
                            let val_str = match v {
                                HelheimType::String(s) => s.clone(),
                                _ => v.to_string(),
                            };
                            let _ = context.set_value(k.clone(), val_str.into());
                        }
                    }
                }
            }

            let eval_str = expr_clean
                .replace(" en ", " && ")
                .replace(" of ", " || ")
                .replace("niet ", "!");

            match evalexpr::eval_with_context(&eval_str, &context) {
                Ok(result) => {
                    match result {
                        evalexpr::Value::Int(i) => return format!("{}", i),
                        evalexpr::Value::Float(f) => return format!("{}", f),
                        evalexpr::Value::Boolean(b) => {
                            return (if b { "waar" } else { "onwaar" }).to_string();
                        }
                        evalexpr::Value::String(s) => return s.clone(),
                        evalexpr::Value::Tuple(t) => {
                            // Serialize Tuple to a JSON array string for Helheim's internal representation
                            let mut json_arr = "[".to_string();
                            for (i, v) in t.iter().enumerate() {
                                if i > 0 {
                                    json_arr.push_str(", ");
                                }
                                match v {
                                    evalexpr::Value::Int(ni) => json_arr.push_str(&ni.to_string()),
                                    evalexpr::Value::Float(nf) => {
                                        json_arr.push_str(&nf.to_string())
                                    }
                                    evalexpr::Value::String(ns) => {
                                        json_arr.push_str(&format!("\"{}\"", ns))
                                    }
                                    _ => json_arr.push_str("\"complex_type\""),
                                }
                            }
                            json_arr.push(']');
                            return json_arr;
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    if (err_str.contains("Expected") || err_str.contains("wrong combination of types")) && (err_str.contains("String") || err_str.contains("Int") || err_str.contains("Float")) {
                        println!("{}", format!("\n[SYNTAX HULP]: Fout in de berekening: '{}'", expr_clean).yellow());
                        println!("{}", format!("  -> Je probeert tekst (String) en getallen (Int/Float) direct te combineren.").yellow());
                        println!("{}", format!("  -> In de nieuwe Native Type engine is dit niet toegestaan ter bescherming van de runtime.").yellow());
                        println!("{}", format!("  -> Oplossing: Houd berekeningen en tekst gescheiden, of bouw een tekst() formatter (komt in volgende update).").cyan());
                    } else if (!err_str.contains("Variable identifier is not bound")
                        && !err_str.contains("Tried to append a node"))
                        || !expr_clean.contains("[")
                    {
                        println!(
                            "[DEBUG]: evalexpr gaf fout op '{}': {}",
                            expr_clean, err_str
                        );
                    }
                }
            }
        }

        // Fallback: return as is (maybe it's just a value or string)
        self.memory.resolve_value(expr)
    }

}
