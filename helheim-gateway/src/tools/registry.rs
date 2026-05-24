//! Tool registry — central place for tool definitions, parsing, and execution dispatch.

use serde::{Deserialize, Serialize};
use tracing::info;

use super::finance;
use super::data;
use super::utility;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_id: String,
    pub success: bool,
    pub output: String,
}

impl ToolResult {
    pub fn ok(id: &str, output: String) -> Self {
        Self { tool_id: id.to_string(), success: true, output }
    }
    pub fn err(id: &str, output: String) -> Self {
        Self { tool_id: id.to_string(), success: false, output }
    }
}

/// All available tools. Add new tools here — one line each.
pub fn available_tools() -> Vec<ToolDef> {
    vec![
        // Finance
        tool("stock_price",  "Aandelenkoers",    "Live aandelenkoersen (bijv: AAPL, TSLA, ASML)", "finance"),
        tool("crypto_price", "Crypto Prijs",     "Live cryptocurrency prijzen (bijv: bitcoin, ethereum)", "finance"),
        tool("forex",        "Wisselkoers",      "Valuta wisselkoersen (bijv: EUR/USD)", "finance"),
        // Data
        tool("web_search",   "Web Zoeken",       "Zoek informatie op het internet", "data"),
        tool("fetch_url",    "URL Ophalen",       "Haal de inhoud van een webpagina op", "data"),
        tool("weather",      "Weer",             "Huidig weer voor een locatie", "data"),
        tool("news",         "Nieuws",           "Laatste nieuws over een onderwerp", "data"),
        // Utility
        tool("calculator",   "Rekenmachine",     "Wiskundige berekeningen", "utility"),
        tool("datetime",     "Datum & Tijd",     "Huidige datum, tijd, tijdzones", "utility"),
        tool("translate",    "Vertalen",         "Vertaal tekst tussen talen", "utility"),
        tool("summarize",    "Samenvatten",      "Vat lange tekst samen", "utility"),
        tool("convert",      "Eenheden",         "Converteer eenheden (km→mi, kg→lb, °C→°F)", "utility"),
    ]
}

fn tool(id: &str, name: &str, desc: &str, cat: &str) -> ToolDef {
    ToolDef { id: id.into(), name: name.into(), description: desc.into(), category: cat.into() }
}

/// Execute a tool by ID
pub async fn execute_tool(tool_id: &str, param: &str, _config: &serde_json::Value) -> ToolResult {
    info!("[TOOLS] {}({})", tool_id, param);
    match tool_id {
        "stock_price"  => finance::stock_price(param).await,
        "crypto_price" => finance::crypto_price(param).await,
        "forex"        => finance::forex(param).await,
        "web_search"   => data::web_search(param).await,
        "fetch_url"    => data::fetch_url(param).await,
        "weather"      => data::weather(param).await,
        "news"         => data::news(param).await,
        "calculator"   => utility::calculator(param),
        "datetime"     => utility::datetime(param),
        "translate"    => utility::translate(param),
        "summarize"    => utility::summarize(param),
        "convert"      => utility::convert(param),
        _ => ToolResult::err(tool_id, format!("Onbekende tool: {}", tool_id)),
    }
}

/// Parse [TOOL_CALL: tool_name(param)] from AI response
pub fn parse_tool_calls(response: &str) -> Vec<(String, String)> {
    let mut calls = Vec::new();
    for line in response.lines() {
        let t = line.trim();
        if t.starts_with("[TOOL_CALL:") && t.ends_with(']') {
            let inner = t[11..t.len()-1].trim();
            if let Some(p) = inner.find('(') {
                let id = inner[..p].trim().to_string();
                let param = inner[p+1..].trim_end_matches(')').trim().to_string();
                calls.push((id, param));
            }
        }
    }
    calls
}

/// Build system prompt with tool instructions + FAQ
pub fn build_tool_prompt(base: &str, faq: &str, enabled: &[String]) -> String {
    let tools = available_tools();
    let active: Vec<&ToolDef> = tools.iter().filter(|t| enabled.contains(&t.id)).collect();

    if active.is_empty() {
        return if faq.is_empty() { base.to_string() } else { format!("{}\n\n--- FAQ ---\n{}", base, faq) };
    }

    let list: String = active.iter().map(|t| format!("- {}: {}", t.id, t.description)).collect::<Vec<_>>().join("\n");

    let mut prompt = format!(
        "{}\n\n## Tools\nJe hebt deze tools. Gebruik ze ALLEEN als je realtime/externe data nodig hebt.\nFormaat (eigen regel): [TOOL_CALL: tool_naam(parameter)]\n\n{}\n\nRegels: gebruik max 1 tool per antwoord. Geen tool nodig? Antwoord gewoon normaal. Altijd Nederlands, vriendelijk.",
        base, list
    );
    if !faq.is_empty() {
        prompt.push_str(&format!("\n\n--- FAQ ---\n{}", faq));
    }
    prompt
}
