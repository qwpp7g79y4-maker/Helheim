//! Finance tools: stocks, crypto, forex

use super::registry::ToolResult;

pub async fn stock_price(ticker: &str) -> ToolResult {
    let ticker = ticker.trim().to_uppercase();
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5d", ticker);
    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                let meta = &json["chart"]["result"][0]["meta"];
                let price = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);
                let prev = meta["chartPreviousClose"].as_f64().unwrap_or(0.0);
                let cur = meta["currency"].as_str().unwrap_or("USD");
                if price == 0.0 { return ToolResult::err("stock_price", format!("Ticker '{}' niet gevonden", ticker)); }
                let pct = if prev > 0.0 { (price - prev) / prev * 100.0 } else { 0.0 };
                let dir = if pct >= 0.0 { "+" } else { "" };
                ToolResult::ok("stock_price", format!("{}: {:.2} {} ({}{:.2}%)", ticker, price, cur, dir, pct))
            }
            _ => ToolResult::err("stock_price", format!("Kon {} niet ophalen", ticker)),
        },
        Err(e) => ToolResult::err("stock_price", format!("Fout: {}", e)),
    }
}

pub async fn crypto_price(coin: &str) -> ToolResult {
    let coin = coin.trim().to_lowercase();
    let url = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=eur,usd&include_24hr_change=true", coin);
    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                if let Some(d) = json.get(&coin) {
                    let eur = d["eur"].as_f64().unwrap_or(0.0);
                    let usd = d["usd"].as_f64().unwrap_or(0.0);
                    let chg = d["eur_24h_change"].as_f64().unwrap_or(0.0);
                    let dir = if chg >= 0.0 { "+" } else { "" };
                    ToolResult::ok("crypto_price", format!("{}: €{:.2} / ${:.2} ({}{:.2}% 24u)", coin, eur, usd, dir, chg))
                } else {
                    ToolResult::err("crypto_price", format!("'{}' niet gevonden. Gebruik volledige naam: bitcoin, ethereum, etc.", coin))
                }
            }
            _ => ToolResult::err("crypto_price", "Kon data niet verwerken".into()),
        },
        Err(e) => ToolResult::err("crypto_price", format!("Fout: {}", e)),
    }
}

pub async fn forex(pair: &str) -> ToolResult {
    // Parse "EUR/USD" or "EURUSD" or "eur usd"
    let clean = pair.trim().to_uppercase().replace(['/', ' ', '-'], "");
    if clean.len() < 6 {
        return ToolResult::err("forex", "Gebruik formaat: EUR/USD of EURUSD".into());
    }
    let from = &clean[..3];
    let to = &clean[3..6];
    // Use Yahoo Finance for forex
    let symbol = format!("{}{}=X", from, to);
    let url = format!("https://query1.finance.yahoo.com/v8/finance/chart/{}?interval=1d&range=5d", symbol);
    match reqwest::get(&url).await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => {
                let meta = &json["chart"]["result"][0]["meta"];
                let price = meta["regularMarketPrice"].as_f64().unwrap_or(0.0);
                let prev = meta["chartPreviousClose"].as_f64().unwrap_or(0.0);
                if price == 0.0 { return ToolResult::err("forex", format!("{}/{} niet gevonden", from, to)); }
                let pct = if prev > 0.0 { (price - prev) / prev * 100.0 } else { 0.0 };
                let dir = if pct >= 0.0 { "+" } else { "" };
                ToolResult::ok("forex", format!("{}/{}: {:.4} ({}{:.2}%)", from, to, price, dir, pct))
            }
            _ => ToolResult::err("forex", "Kon wisselkoers niet ophalen".into()),
        },
        Err(e) => ToolResult::err("forex", format!("Fout: {}", e)),
    }
}
