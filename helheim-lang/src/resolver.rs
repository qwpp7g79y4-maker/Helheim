// helheim-lang/src/resolver.rs
// Minimal but real compile-time module linker for Helheim CodeTaal.
// Must run immediately after HelParser::parse and before SemanticAnalyzer.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::ast::CodeTaal;
use crate::parser::HelParser;

/// Compile-time module resolver / linker.
/// Expands `CodeTaal::Gebruik` nodes by reading and parsing the referenced .hel files
/// and inlining their statements (like a clean #include at AST level).
pub struct ModuleLinker {
    /// Directories to search for modules (project dir first, then std lib).
    search_paths: Vec<PathBuf>,
    /// Canonical paths of already expanded modules (prevents cycles and duplicate work).
    loaded: HashSet<PathBuf>,
}

impl ModuleLinker {
    /// Create a linker with explicit search paths.
    /// The first path should typically be the directory of the entry file.
    pub fn new(search_paths: Vec<PathBuf>) -> Self {
        Self {
            search_paths,
            loaded: HashSet::new(),
        }
    }

    /// Convenience constructor for typical usage.
    /// - `entry_dir`: directory of the main .hel file being compiled
    /// - `std_lib_dir`: e.g. `~/.helheim/lib` or a project-local `lib/` directory
    pub fn with_std_lib(entry_dir: PathBuf, std_lib_dir: PathBuf) -> Self {
        let mut paths = vec![entry_dir];
        paths.push(std_lib_dir);
        Self::new(paths)
    }

    /// Top-level entry point.
    /// Takes the raw AST from the main file and returns a fully expanded AST
    /// with all `Gebruik` nodes replaced by their contents.
    /// This must be called **before** semantic analysis and lowering.
    pub fn link(&mut self, main_ast: Vec<CodeTaal>, entry_file: &Path) -> Result<Vec<CodeTaal>> {
        let entry_dir = entry_file
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        // Make sure the entry directory is first in search paths
        if !self.search_paths.iter().any(|p| p == &entry_dir) {
            self.search_paths.insert(0, entry_dir.clone());
        }

        self.expand(main_ast, &entry_dir)
    }

    /// Recursively expands Gebruik statements.
    fn expand(&mut self, statements: Vec<CodeTaal>, current_dir: &Path) -> Result<Vec<CodeTaal>> {
        let mut out = Vec::with_capacity(statements.len() * 2);

        for stmt in statements {
            if let CodeTaal::Gebruik { path } = stmt {
                let module_file = self.resolve_module_path(&path, current_dir)?;

                // Already expanded (cycle protection or duplicate import)
                if !self.loaded.insert(module_file.clone()) {
                    continue;
                }

                let source = std::fs::read_to_string(&module_file)
                    .with_context(|| format!("Failed to read module '{}'", module_file.display()))?;

                let module_ast = HelParser::parse(&source)
                    .with_context(|| format!("Failed to parse module '{}'", module_file.display()))?;

                let module_dir = module_file
                    .parent()
                    .unwrap_or(current_dir)
                    .to_path_buf();

                let expanded = self.expand(module_ast, &module_dir)?;
                out.extend(expanded);
            } else {
                out.push(stmt);
            }
        }

        Ok(out)
    }

    /// Resolves a module specifier (e.g. "core", "math", "utils/helpers") to a real file.
    /// Priority:
    /// 1. Special handling for "core" and core/* (maps to std lib)
    /// 2. Relative to current file
    /// 3. Search paths
    fn resolve_module_path(&self, module: &str, current_dir: &Path) -> Result<PathBuf> {
        // Special case: standard library modules
        if module == "core" || module.starts_with("core/") {
            let rel = if module == "core" {
                "core.hel"
            } else {
                module.strip_prefix("core/").unwrap()
            };
            // Try to find core.hel or core/<rel>.hel under any search path that looks like std
            for base in &self.search_paths {
                let candidate = if module == "core" {
                    base.join("core.hel")
                } else {
                    base.join("core").join(rel).with_extension("hel")
                };
                if candidate.exists() {
                    return Ok(candidate);
                }
            }
            // Fallback: last search path (usually the std lib dir)
            if let Some(last) = self.search_paths.last() {
                let candidate = if module == "core" {
                    last.join("core.hel")
                } else {
                    last.join("core").join(rel).with_extension("hel")
                };
                return Ok(candidate);
            }
        }

        // Normal module: try relative to current file first
        let mut candidates = vec![
            current_dir.join(module).with_extension("hel"),
            current_dir.join(module),
        ];

        // Then search paths
        for base in &self.search_paths {
            candidates.push(base.join(module).with_extension("hel"));
            candidates.push(base.join(module));
        }

        for c in candidates {
            if c.exists() && c.is_file() {
                return Ok(c.canonicalize().unwrap_or(c));
            }
        }

        anyhow::bail!("Module '{}' not found (searched relative to {} and search paths)", module, current_dir.display())
    }
}
