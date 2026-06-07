// CONCEPTS/executor_functions_waterdicht.rs
// Prioriteit 1 - Executor logica voor waterdichte functies + diepe returns
// Betreft: helheim-core/src/orchestra/executor.rs  +  helheim-lang/src/memory.rs
//
// Antigravity: 
// 1. Eerst de ScopeGuard toevoegen in helheim-lang/src/memory.rs
// 2. Dan execute_function_call + propagate_return updaten in de executor.
// 3. Optioneel de control-flow armen (Loop/If) iets netter maken.

use helheim_lang::ast::CodeTaal;
use crate::orchestra::memory::{MemoryManager, HelheimType};
use anyhow::Result;

// ============================================================
// DEEL 1: ScopeGuard in helheim-lang/src/memory.rs
// ============================================================

// Voeg dit toe (bij voorkeur direct na de MemoryManager struct of aan het eind van het bestand).

/// RAII ScopeGuard.
/// Garandeert dat een lokale scope (voor een functie-aanroep) altijd wordt gepopt,
/// ook bij early return, error (`?`), of panic.
pub struct ScopeGuard<'a> {
    memory: &'a MemoryManager,
    active: bool,
}

impl<'a> ScopeGuard<'a> {
    /// Pusht direct een nieuwe scope en retourneert een guard die bij drop zal poppen.
    pub fn new(memory: &'a MemoryManager) -> Self {
        memory.push_scope();
        Self { memory, active: true }
    }

    /// Vroege pop (bijv. als je de guard expliciet wilt opruimen na een return).
    /// Na deze call doet Drop niets meer.
    pub fn pop_now(mut self) {
        if self.active {
            self.memory.pop_scope();
            self.active = false;
        }
    }
}

impl Drop for ScopeGuard<'_> {
    fn drop(&mut self) {
        if self.active {
            self.memory.pop_scope();
        }
    }
}

// De bestaande push_scope / pop_scope blijven bestaan (worden intern gebruikt door de guard).
// Je kunt ze eventueel private maken als je wilt forceren dat alleen de guard gebruikt wordt.

// ============================================================
// DEEL 2: Executor changes (helheim-core/src/orchestra/executor.rs)
// ============================================================

// Voeg de propagate_return helper toe (of houd hem zoals hij nu is).

impl Executor {
    /// Centrale helper voor return-propagatie uit geneste scopes.
    /// Iedere `retourneer` (op elk diepteniveau) zorgt ervoor dat execute_ast
    /// een Ok(Some(value)) teruggeeft. Deze helper + de early returns in
    /// de Loop/If/ForEach/TryCatch armen zorgen voor de directe unwind.
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
        // ... eventuele native intercepts (tekst, nummer, etc.) ...

        // 1. Native library
        if let Some(res) = system::SystemManager::try_execute_native(&self.memory, name, &args, &ctx).await? {
            return Ok(res);
        }

        // 2. User-defined AST functie (pure CodeTaal pad)
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

            // =====================================================
            // DE KERN VAN LEAK-PROOF + DIEPE RETURN SUPPORT
            // =====================================================
            // De ScopeGuard pusht de scope en popt hem gegarandeerd (Drop).
            // Zelfs als propagate_return een diepe return oplevert of er een error optreedt.
            let _scope_guard = ScopeGuard::new(&self.memory);

            for (i, param) in params.iter().enumerate() {
                self.memory.set_var_native(param.clone(), HelheimType::parse(&resolved_args[i]));
            }

            // Diepe return uit zolang/als/etc. wordt hier opgevangen.
            // De guard zorgt voor de pop, ongeacht of we hier of in de Err-tak uitkomen.
            let result = match self.propagate_return(&body, ctx.clone()).await {
                Ok(Some(ret)) => ret,
                Ok(None) => "".to_string(),
                Err(e) => {
                    // We hoeven hier geen pop meer te doen — de guard doet het in Drop.
                    return Err(e);
                }
            };

            // Normaal pad: guard dropt hier en doet de pop.
            Ok(result)
        } else {
            println!("[ERR]: Functie '{}' bestaat niet in AST store of Native Library.", name);
            Ok("".to_string())
        }
    }
}

// ============================================================
// Optionele netheid: gebruik propagate_return ook in de control arms
// (reeds deels aanwezig, hier de compacte versie voor Loop en If)
// ============================================================

// In de execute_ast match-arm voor Loop:
/*
CodeTaal::Loop { condition, body } => {
    let mut iterations = 0;
    loop {
        let should_run = self.evaluate_ast_condition(&condition, ctx.clone()).await;
        if !should_run || iterations > 1000 {
            break;
        }
        if let Some(ret) = self.propagate_return(&body, ctx.clone()).await? {
            return Ok(Some(ret));   // directe unwind uit de functie
        }
        iterations += 1;
    }
}
*/

// Voor If:
/*
CodeTaal::If { condition, then, else_block } => {
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
*/

// Dezelfde pattern is nuttig voor ForEach en de catch-block van TryCatch.

// ============================================================
// Samenvatting van de garanties die dit oplevert
// ============================================================
//
// 1. Scope leak prevention:
//    - ScopeGuard gebruikt Drop → pop gebeurt altijd, ook bij `?` of panic.
//
// 2. Diepe returns:
//    - `retourneer <expr>` in een diep geneste zolang of als produceert
//      Ok(Some(evaluated_value)) uit execute_ast.
//    - propagate_return + de early `return Ok(Some(ret))` in de control arms
//      breken de hele functie-body af en geven de waarde terug aan de caller
//      van roep_aan / FunctionCall.
//
// 3. Werkt met de huidige DashMap migratie:
//    - Globals zitten in DashMap (lock-free reads).
//    - local_stack blijft een Vec<HashMap> (bewuste stack discipline).
//    - get_var_native zoekt eerst in de local_stack (van achteren naar voren),
//      dan in globals. Dit gedrag verandert niet.
//
// 4. Compatibel met zowel ; als newline-only stijl (mits de parser-fix ook wordt toegepast).
