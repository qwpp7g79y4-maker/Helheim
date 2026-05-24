//! Helheim Bot Tools — Modulair tool systeem
//!
//! Elke tool implementeert de `Tool` trait.
//! Nieuwe tool toevoegen = 1 bestand + 1 regel in `registry()`.

mod registry;
mod finance;
mod data;
mod utility;
mod templates;

pub use registry::{ToolDef, ToolResult, available_tools, execute_tool, parse_tool_calls, build_tool_prompt};
pub use templates::{BotTemplate, bot_templates};
