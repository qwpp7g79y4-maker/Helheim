// CONCEPTS/parser_functions_waterdicht.rs
// Prioriteit 1 - Parser logica voor waterdichte functies
// Bestand: helheim-lang/src/parser.rs
//
// Dit is de minimale, gerichte change die voorkomt dat een "zet x = foo"
// (zonder ;) de daaropvolgende "zolang", "als", "retourneer" etc. opslokt
// in de functie-body.
//
// Antigravity: kopieer de "zet" arm hieronder en vervang de bestaande
// collector in parse_statement.

use crate::ast::{CodeTaal, LiteralValue};
use anyhow::Result;
use std::iter::Peekable;

// ... bestaande imports en Token struct ...

impl HelParser {

    // === VERVANG DEZE ARM IN parse_statement (de "zet" | "let" | "set" match arm) ===

    "zet" | "let" | "set" => {
        let name = iter
            .next()
            .ok_or(Self::format_parse_error(input, &token, "Verwachte variabele naam na 'zet'"))?;
        let eq = iter
            .next()
            .ok_or(Self::format_parse_error(input, &token, "Verwachte '=' na variabele"))?;
        if eq != "=" {
            return Err(Self::format_parse_error(
                input,
                &eq,
                &format!("Syntax fout: verwachte '=', gevonden '{}'", eq.value),
            ));
        }

        // === BELANGRIJKSTE VERBETERING VOOR FUNCTIES ===
        // Oude collector slurpte control-flow statements op.
        // Nieuwe versie stopt expliciet vóór statement-keywords op brace-niveau 0.
        let mut val_tokens = Vec::new();
        let mut brace_count = 0;

        while let Some(t) = iter.peek() {
            if t == ";" {
                break;
            }

            // Stop vóór een nieuw top-level statement als we geen open { hebben.
            // Dit is de key fix voor "diepe returns in functies".
            if brace_count == 0 && matches!(
                t.value.as_str(),
                "zolang" | "while" | "repeat" |
                "als" | "if" |
                "retourneer" | "geef_terug" | "return" |
                "zet" | "let" | "set" |
                "functie" | "fn" | "func" |
                "voor" | "for" |
                "probeer" | "try" |
                "schrijf" | "print" | "log"
            ) {
                break;
            }

            if t == "{" {
                brace_count += 1;
            }
            if t == "}" {
                if brace_count == 0 {
                    break;
                }
                brace_count -= 1;
            }

            val_tokens.push(iter.next().ok_or_else(|| {
                Self::format_parse_error(input, &token, "Onverwacht einde van het script")
            })?);
        }

        let expr = if val_tokens.is_empty() {
            return Err(anyhow::anyhow!(
                "Fout op regel {}: Verwachte waarde voor '{}'",
                token.line, name
            ));
        } else if val_tokens.len() == 1 {
            let v = &val_tokens[0].value;
            if v.parse::<i64>().is_ok() {
                Box::new(CodeTaal::Literal(LiteralValue::Int(v.parse().unwrap())))
            } else if v.parse::<f64>().is_ok() {
                Box::new(CodeTaal::Literal(LiteralValue::Float(v.parse().unwrap())))
            } else if v.starts_with("\"") {
                let s = v.trim_matches('"').to_string();
                Box::new(CodeTaal::Literal(LiteralValue::String(s)))
            } else if v == "true" || v == "false" || v == "waar" || v == "onwaar" {
                Box::new(CodeTaal::Literal(LiteralValue::Bool(v == "true" || v == "waar")))
            } else {
                Box::new(CodeTaal::VarGet { name: v.clone() })
            }
        } else {
            let mut expr_iter = val_tokens.into_iter().peekable();
            Box::new(Self::parse_expression(input, &mut expr_iter, 0)?)
        };

        // Optioneel ; consumeren (bestaande pattern)
        if let Some(next) = iter.peek() {
            if next == ";" {
                iter.next();
            }
        }

        Ok(Some(CodeTaal::VarDef { name: name.value.clone(), value: expr }))
    }

    // === De rest van de functie-parsing (functie / fn arm) blijft grotendeels hetzelfde ===

    "functie" | "func" | "fn" | "function" => {
        let name = iter.next().ok_or(Self::format_parse_error(input, &token, "Verwacht functienaam"))?;
        let mut params = Vec::new();
        while let Some(t) = iter.peek() {
            if t == "{" { break; }
            if t == "met" || t == "with" || t == "," {
                iter.next();
                continue;
            }
            params.push(iter.next().ok_or_else(|| {
                Self::format_parse_error(input, &token, "Onverwacht einde van het script")
            })?.value);
        }
        let body_ast = Box::new(Self::parse_block(input, iter)?);
        Ok(Some(CodeTaal::FunctionDef {
            name: name.value.clone(),
            params,
            body: body_ast,
        }))
    }

    // Return parsing is al goed (gebruikt parse_expression)
    "geef_terug" | "retourneer" | "return" => {
        let mut val_tokens = Vec::new();
        while let Some(t) = iter.peek() {
            if t == ";" || t == "}" { break; }
            val_tokens.push(iter.next().ok_or_else(|| {
                Self::format_parse_error(input, &token, "Onverwacht einde van het script")
            })?);
        }
        let value = if val_tokens.is_empty() {
            None
        } else {
            let mut expr_iter = val_tokens.into_iter().peekable();
            Some(Box::new(Self::parse_expression(input, &mut expr_iter, 0)?))
        };
        Ok(Some(CodeTaal::Return { value }))
    }
}

// parse_block en de zolang/als armen hoeven niet te veranderen.
// De fix in de zet-collector zorgt dat de statements netjes als siblings
// in de FunctionDef body Block terechtkomen.
