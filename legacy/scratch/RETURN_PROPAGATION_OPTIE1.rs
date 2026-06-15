// RETURN_PROPAGATION_OPTIE1.rs
// Compact, exact Rust code for Return propagation in nested scopes (Fase 1.2 / Optie 1)
// For helheim-lang / helheim-core pure CPU CodeTaal execution.
//
// Drop this logic into helheim-core/src/orchestra/executor.rs (Executor impl).
// Then `cargo test -p helheim-core --test integration_tests test_general_pure_functions`
//
// Key properties:
// - Any `retourneer` / `geef_terug` / `return` from deep inside `als` / `zolang` (or try/foreach)
//   causes execute_ast(...) to return Ok(Some(value)).
// - Control structures (Loop/If/ForEach/TryCatch) use propagate_return to immediately unwind.
// - Function entry point (execute_function_call) catches it as the call result.
// - Scope (push/pop for function locals) is *always* cleaned, even on error or deep return.
// - No leaks, direct call-stack break back to the original caller (script, another fn, or expr).

use helheim_lang::ast::CodeTaal;
// ... other uses ...

impl Executor {
    /// Compact helper for Return propagation within nested scopes.
    /// When a Return node is hit at any depth, the corresponding execute_ast call
    /// yields Ok(Some(the_value)). Callers of this helper (or direct early returns)
    /// abort the current execution frame and bubble the value up to the function boundary.
    async fn propagate_return(
        &self,
        body: &CodeTaal,
        ctx: crate::common::context::ExecutionContext,
    ) -> Result<Option<String>> {
        match body {
            CodeTaal::Block { statements } => self.execute_ast(statements.clone(), ctx).await,
            other => self.execute_ast(vec![other.clone()], ctx).await,
        }
    }

    async fn execute_function_call(
        &self,
        name: &str,
        args: Vec<String>,
        ctx: crate::common::context::ExecutionContext,
    ) -> Result<String> {
        // 1. Native
        if let Some(res) = system::SystemManager::try_execute_native(&self.memory, name, &args, &ctx).await? {
            return Ok(res);
        }

        // 2. User AST function (the pure general CodeTaal path)
        let func_tuple = {
            let store = self.memory.ast_funcs.lock().unwrap_or_else(|e| e.into_inner());
            store.get(name).cloned()
        };

        if let Some((params, body)) = func_tuple {
            let mut resolved_args = Vec::new();
            for i in 0..params.len() {
                if i < args.len() {
                    resolved_args.push(self.memory.resolve_value(&args[i]));
                } else {
                    resolved_args.push("".to_string());
                }
            }

            // Open function scope for locals/params. Must be popped on every exit path.
            self.memory.push_scope();

            for (i, param) in params.iter().enumerate() {
                self.memory.set_var_native(param.clone(), HelheimType::parse(&resolved_args[i]));
            }

            // === THE CORE OF ROBUST RETURN PROPAGATION ===
            // propagate_return delegates to execute_ast on the body.
            // Inside execute_ast:
            //   - Direct Return stmt => return Ok(Some(evaluated))
            //   - If / Loop / ForEach / TryCatch use propagate_return (or equivalent early return)
            //     on their sub-bodies. A deep return therefore immediately returns Ok(Some(..))
            //     from the whole body execute_ast, aborting any remaining statements/loops.
            // The value is delivered here as the result of the roep_aan / call.
            let result = match self.propagate_return(&body, ctx.clone()).await {
                Ok(Some(ret)) => ret,
                Ok(None) => "".to_string(),
                Err(e) => {
                    self.memory.pop_scope();
                    return Err(e);
                }
            };

            self.memory.pop_scope();
            Ok(result)
        } else {
            println!("[ERR]: Functie '{}' bestaat niet in AST store of Native Library.", name);
            Ok("".to_string())
        }
    }

    // Example usage sites inside execute_ast (control flow must forward the signal):
    //
    // CodeTaal::Loop { condition, body } => {
    //     ...
    //     if let Some(ret) = self.propagate_return(&body, ctx.clone()).await? {
    //         return Ok(Some(ret));   // aborts the zolang, returns from function
    //     }
    //     ...
    // }
    //
    // CodeTaal::If { condition, then, else_block } => {
    //     if cond {
    //         if let Some(ret) = self.propagate_return(&then, ctx.clone()).await? {
    //             return Ok(Some(ret));
    //         }
    //     } else if let Some(eb) = else_block {
    //         if let Some(ret) = self.propagate_return(&eb, ctx.clone()).await? {
    //             return Ok(Some(ret));
    //         }
    //     }
    // }
    //
    // Same pattern for ForEach and TryCatch catch_block.
    //
    // At top of execute_ast the Return arm already does:
    // CodeTaal::Return { value } => {
    //     let eval = ... evaluate_ast_expr(value) ...;
    //     return Ok(Some(eval));
    // }
}

// Integration & test
// ------------------
// 1. Place the propagate_return helper + updated execute_function_call in the impl Executor.
// 2. Update the four control arms (Loop, If, ForEach, TryCatch) to call propagate_return
//    on their body/then/else/catch and do `if let Some(ret) = ... { return Ok(Some(ret)); }`.
// 3. cargo test -p helheim-core --test integration_tests test_general_pure_functions
//    (both the simple one and the deep_return variant that does return from inside zolang+als must pass).
// 4. Optionally run the helheim-cli on examples/language/helheim_nexus_breach.hel
//
// This gives exactly the "direct de call stack afbreekt + geen scope-leaks" behaviour
// requested for functions in the pure CPU-taal.