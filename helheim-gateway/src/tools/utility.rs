//! Utility tools: calculator, datetime, translate, summarize, convert

use super::registry::ToolResult;

pub fn calculator(expr: &str) -> ToolResult {
    let expr = expr.trim();
    match eval(expr) {
        Some(v) => ToolResult::ok("calculator", format!("{} = {}", expr, v)),
        None => ToolResult::err("calculator", format!("Kon '{}' niet berekenen", expr)),
    }
}

pub fn datetime(_param: &str) -> ToolResult {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let nl_h = (h + 1) % 24; // CET = UTC+1
    let days = secs / 86400;
    let (y, mo, d) = days_to_date(days);
    let wd = ["do","vr","za","zo","ma","di","wo"][(days % 7) as usize];
    ToolResult::ok("datetime", format!("{} {}-{:02}-{:02} | NL: {:02}:{:02} | UTC: {:02}:{:02}", wd, y, mo, d, nl_h, m, h, m))
}

pub fn translate(param: &str) -> ToolResult {
    ToolResult::ok("translate", format!("Vertaalverzoek: {}. Gebruik je taalkennis om te vertalen.", param))
}

pub fn summarize(param: &str) -> ToolResult {
    ToolResult::ok("summarize", format!("Samenvatverzoek: {}. Vat de kern samen in max 3 zinnen.", param))
}

pub fn convert(param: &str) -> ToolResult {
    let p = param.trim().to_lowercase();
    // Parse: "100 km to mi", "25 celsius to fahrenheit", "10 kg to lb"
    let parts: Vec<&str> = p.split_whitespace().collect();
    if parts.len() < 3 {
        return ToolResult::err("convert", "Formaat: '100 km to mi' of '25 celsius to fahrenheit'".into());
    }
    let val: f64 = match parts[0].parse() {
        Ok(v) => v,
        Err(_) => return ToolResult::err("convert", format!("'{}' is geen getal", parts[0])),
    };
    let from = parts[1];
    let to = parts.last().unwrap_or(&"");

    let result = match (from, *to) {
        ("km", "mi") | ("km", "miles") => Some((val * 0.621371, "mi")),
        ("mi", "km") | ("miles", "km") => Some((val * 1.60934, "km")),
        ("kg", "lb") | ("kg", "lbs") | ("kg", "pounds") => Some((val * 2.20462, "lb")),
        ("lb", "kg") | ("lbs", "kg") | ("pounds", "kg") => Some((val * 0.453592, "kg")),
        ("m", "ft") | ("meter", "feet") => Some((val * 3.28084, "ft")),
        ("ft", "m") | ("feet", "meter") => Some((val * 0.3048, "m")),
        ("cm", "inch") | ("cm", "in") => Some((val * 0.393701, "inch")),
        ("inch", "cm") | ("in", "cm") => Some((val * 2.54, "cm")),
        ("celsius", "fahrenheit") | ("c", "f") => Some((val * 9.0/5.0 + 32.0, "°F")),
        ("fahrenheit", "celsius") | ("f", "c") => Some(((val - 32.0) * 5.0/9.0, "°C")),
        ("l", "gal") | ("liter", "gallon") => Some((val * 0.264172, "gal")),
        ("gal", "l") | ("gallon", "liter") => Some((val * 3.78541, "L")),
        ("eur", "usd") => Some((val * 1.08, "$")), // approximate
        ("usd", "eur") => Some((val * 0.926, "€")),
        _ => None,
    };

    match result {
        Some((r, unit)) => ToolResult::ok("convert", format!("{} {} = {:.2} {}", val, from, r, unit)),
        None => ToolResult::err("convert", format!("Kan '{}' → '{}' niet converteren", from, to)),
    }
}

// Simple recursive math evaluator
fn eval(expr: &str) -> Option<f64> {
    let e = expr.replace(',', ".").replace(' ', "");
    if let Ok(n) = e.parse::<f64>() { return Some(n); }
    // Handle parentheses
    if e.starts_with('(') && e.ends_with(')') {
        return eval(&e[1..e.len()-1]);
    }
    // Find lowest-precedence operator (right to left: +-, then */, then ^)
    for ops in &[&['+', '-'][..], &['*', '/'], &['^', '%']] {
        let mut depth = 0i32;
        let bytes = e.as_bytes();
        for i in (0..bytes.len()).rev() {
            match bytes[i] {
                b')' => depth += 1,
                b'(' => depth -= 1,
                c if depth == 0 && ops.contains(&(c as char)) && i > 0 => {
                    let left = eval(&e[..i])?;
                    let right = eval(&e[i+1..])?;
                    return match c {
                        b'+' => Some(left + right),
                        b'-' => Some(left - right),
                        b'*' => Some(left * right),
                        b'/' => if right != 0.0 { Some(left / right) } else { None },
                        b'^' => Some(left.powf(right)),
                        b'%' => if right != 0.0 { Some(left % right) } else { None },
                        _ => None,
                    };
                }
                _ => {}
            }
        }
    }
    None
}

fn days_to_date(days: u64) -> (u64, u64, u64) {
    let mut y = 1970u64;
    let mut rem = days;
    loop {
        let dy = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 366 } else { 365 };
        if rem < dy { break; }
        rem -= dy;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let md = [31, if leap {29} else {28}, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    while m < 12 && rem >= md[m] { rem -= md[m]; m += 1; }
    (y, (m+1) as u64, (rem+1) as u64)
}
