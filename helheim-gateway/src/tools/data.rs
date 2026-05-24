//! Data tools: web search, fetch URL, weather, news

use super::registry::ToolResult;

pub async fn web_search(query: &str) -> ToolResult {
    let url = format!("https://api.duckduckgo.com/?q={}&format=json&no_html=1&skip_disambig=1", urlencoding::encode(query));
    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                let mut parts = Vec::new();
                if let Some(s) = json["AbstractText"].as_str() { if !s.is_empty() { parts.push(format!("Samenvatting: {}", s)); } }
                if let Some(s) = json["Answer"].as_str() { if !s.is_empty() { parts.push(format!("Antwoord: {}", s)); } }
                if let Some(topics) = json["RelatedTopics"].as_array() {
                    for (i, t) in topics.iter().take(5).enumerate() {
                        if let Some(s) = t["Text"].as_str() { if !s.is_empty() { parts.push(format!("{}. {}", i+1, s)); } }
                    }
                }
                if parts.is_empty() {
                    ToolResult::ok("web_search", format!("Geen directe resultaten voor '{}'. Probeer specifieker.", query))
                } else {
                    ToolResult::ok("web_search", parts.join("\n"))
                }
            }
            _ => ToolResult::err("web_search", "Kon resultaten niet verwerken".into()),
        },
        Err(e) => ToolResult::err("web_search", format!("Fout: {}", e)),
    }
}

pub async fn fetch_url(url: &str) -> ToolResult {
    let url = url.trim();
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return ToolResult::err("fetch_url", "URL moet beginnen met http:// of https://".into());
    }
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().unwrap_or_default();
    match client.get(url).header("User-Agent", "HelheimBot/1.0").send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => {
                let clean = strip_html(&text);
                let out = if clean.len() > 2000 { format!("{}...", &clean[..2000]) } else { clean };
                ToolResult::ok("fetch_url", out)
            }
            _ => ToolResult::err("fetch_url", "Kon pagina niet lezen".into()),
        },
        Err(e) => ToolResult::err("fetch_url", format!("Fout: {}", e)),
    }
}

pub async fn weather(location: &str) -> ToolResult {
    let loc = location.trim();
    let url = format!("https://wttr.in/{}?format=j1", urlencoding::encode(loc));
    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                let c = &json["current_condition"][0];
                let temp = c["temp_C"].as_str().unwrap_or("?");
                let feels = c["FeelsLikeC"].as_str().unwrap_or("?");
                let desc = c["weatherDesc"][0]["value"].as_str().unwrap_or("?");
                let hum = c["humidity"].as_str().unwrap_or("?");
                let wind = c["windspeedKmph"].as_str().unwrap_or("?");
                let dir = c["winddir16Point"].as_str().unwrap_or("?");
                ToolResult::ok("weather", format!(
                    "{}: {}°C (voelt {}°C), {}, vochtigheid {}%, wind {} km/u {}", loc, temp, feels, desc, hum, wind, dir
                ))
            }
            _ => ToolResult::err("weather", format!("Kon weer voor '{}' niet ophalen", loc)),
        },
        Err(e) => ToolResult::err("weather", format!("Fout: {}", e)),
    }
}

pub async fn news(topic: &str) -> ToolResult {
    // DuckDuckGo news via instant answer
    let url = format!("https://api.duckduckgo.com/?q={} nieuws&format=json&no_html=1", urlencoding::encode(topic));
    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                let mut parts = Vec::new();
                if let Some(s) = json["AbstractText"].as_str() { if !s.is_empty() { parts.push(s.to_string()); } }
                if let Some(topics) = json["RelatedTopics"].as_array() {
                    for (i, t) in topics.iter().take(5).enumerate() {
                        if let Some(s) = t["Text"].as_str() { if !s.is_empty() { parts.push(format!("{}. {}", i+1, s)); } }
                    }
                }
                if parts.is_empty() {
                    ToolResult::ok("news", format!("Geen recent nieuws gevonden over '{}'", topic))
                } else {
                    ToolResult::ok("news", parts.join("\n"))
                }
            }
            _ => ToolResult::err("news", "Kon nieuws niet ophalen".into()),
        },
        Err(e) => ToolResult::err("news", format!("Fout: {}", e)),
    }
}

fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    // Collapse whitespace
    let mut result = String::new();
    let mut was_space = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !was_space { result.push(' '); }
            was_space = true;
        } else {
            result.push(c);
            was_space = false;
        }
    }
    result.trim().to_string()
}
