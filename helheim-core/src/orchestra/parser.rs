use crate::orchestra::synthesis::CodeTaal;
use anyhow::Result;
use std::iter::Peekable;

/// De Helheim Parser: Zet 'Helheim' (Naturel) om in Abstracte Logica (AST).
pub struct HelParser;

impl HelParser {
    pub fn parse(input: &str) -> Result<Vec<CodeTaal>> {
        let tokens = Tokenizer::tokenize(input);
        let mut iter = tokens.into_iter().peekable();
        let mut ast = Vec::new();

        while iter.peek().is_some() {
            if let Some(stmt) = Self::parse_statement(&mut iter)? {
                ast.push(stmt);
            }
        }
        Ok(ast)
    }

    fn parse_statement(
        iter: &mut Peekable<std::vec::IntoIter<String>>,
    ) -> Result<Option<CodeTaal>> {
        let token = match iter.next() {
            Some(t) => t,
            None => return Ok(None),
        };

        match token.as_str() {
            "gebruik" | "use" | "import" => {
                let path = iter.next().ok_or(anyhow::anyhow!(
                    "Verwacht een bestandsnaam na 'gebruik' of 'use'"
                ))?;
                // Remove semicolon if attached (tokenizer splits them unless quoted, but just in case)
                let clean_path = path.trim_matches('"').trim_end_matches(';').to_string();

                // Optionele puntkomma consumeren
                if let Some(next_tok) = iter.peek()
                    && next_tok == ";" {
                        iter.next();
                    }

                Ok(Some(CodeTaal::Gebruik { path: clean_path }))
            }
            "zet" | "let" | "set" => {
                // zet [naam] = [waarde]
                let name = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Verwachte variabele naam na 'zet'"))?;
                let eq = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Verwachte '=' na variabele"))?;
                if eq != "=" {
                    return Err(anyhow::anyhow!(
                        "Syntax fout: verwachte '=', gevonden '{}'",
                        eq
                    ));
                }

                // Verbeterde value parser: Lees alles tot ';' of ongebalanceerde '}'
                let mut val_tokens = Vec::new();
                let mut brace_count = 0;
                while let Some(t) = iter.peek() {
                    if t == ";" {
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
                    val_tokens.push(iter.next().unwrap());
                }
                let value = val_tokens.join(" ");
                if value.is_empty() {
                    return Err(anyhow::anyhow!("Verwachte waarde voor '{}'", name));
                }

                Ok(Some(CodeTaal::VarDef { name, value }))
            }
            "zolang" | "while" | "repeat" => {
                // zolang [conditie] { ... }
                // Conditie is alles tot de {
                let mut condition_parts = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    condition_parts.push(iter.next().unwrap());
                }
                let condition_str = condition_parts.join(" ");
                // Wrap condition in a SysOp or specific Condition struct?
                // For now, let's assume condition is a simple check passed to SysOp or VarGet
                // Simplified: condition matches a variable or expression
                let cond_ast = Box::new(CodeTaal::VarGet {
                    name: condition_str,
                }); // Placeholder AST for condition

                // Parse Block
                let body_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::Loop {
                    condition: cond_ast,
                    body: body_ast,
                }))
            }
            "voor" => {
                // voor elke [item] in [LIJST] { ... }
                let elke = iter.next().unwrap_or_default();
                if elke != "elke" {
                    return Err(anyhow::anyhow!("Verwacht 'elke' na 'voor'"));
                }

                let iterator = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Verwacht variabele na 'voor elke'"))?;

                let in_kw = iter.next().unwrap_or_default();
                if in_kw != "in" {
                    return Err(anyhow::anyhow!("Verwacht 'in' na '{}'", iterator));
                }

                // We consume everything till '{' as the iterable string
                let mut iter_parts = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    iter_parts.push(iter.next().unwrap());
                }
                let iterable = iter_parts.join(" ");

                let body_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::ForEach {
                    iterator,
                    iterable,
                    body: body_ast,
                }))
            }
            "als" | "if" => {
                // als [conditie] dan { ... } [anders { ... }]
                let mut condition_parts = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "dan" {
                        iter.next();
                        break;
                    } // Consume 'dan'
                    if t == "{" {
                        break;
                    } // Fallback if 'dan' is missing
                    condition_parts.push(iter.next().unwrap());
                }
                let condition_str = condition_parts.join(" ");
                let cond_ast = Box::new(CodeTaal::VarGet {
                    name: condition_str,
                });

                let body_ast = Box::new(Self::parse_block(iter)?);

                // Optioneel 'anders' blok vangen
                let mut else_block = None;
                if let Some(next_token) = iter.peek()
                    && next_token == "anders" {
                        // Consume 'anders'
                        iter.next();
                        else_block = Some(Box::new(Self::parse_block(iter)?));
                    }

                Ok(Some(CodeTaal::If {
                    condition: cond_ast,
                    then: body_ast,
                    else_block,
                }))
            }
            "tegelijkertijd" => {
                let block_ast = Box::new(Self::parse_block(iter)?);
                let statements = if let CodeTaal::Block { statements } = *block_ast {
                    statements
                } else {
                    Vec::new()
                };
                Ok(Some(CodeTaal::Concurrent { statements }))
            }
            "probeer" => {
                // probeer { ... } vang err { ... }
                let try_ast = Box::new(Self::parse_block(iter)?);

                let vang_token = iter.next().unwrap_or_default();
                if vang_token != "vang" {
                    return Err(anyhow::anyhow!("Verwacht 'vang' na 'probeer'-blok"));
                }

                let mut error_var = None;
                if let Some(t) = iter.peek()
                    && t != "{" {
                        error_var = Some(iter.next().unwrap().to_string());
                    }

                let catch_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::TryCatch {
                    try_block: try_ast,
                    catch_block: catch_ast,
                    error_var,
                }))
            }
            "stuur" => {
                // stuur [bericht] naar [targets...]
                // Dit is complexer met tokens.
                // We reconstrueren de zin en gebruiken de bestaande regex/split logic in CodeTaal::Send?
                // Nee, parser moet het doen.
                let payload = iter.next().unwrap_or_default();
                // Als payload tussen quotes staat, is het 1 token.

                let mut targets = Vec::new();
                if let Some(naar) = iter.next()
                    && naar == "naar" {
                        while let Some(t) = iter.peek() {
                            if t == ";" || t == "}" {
                                break;
                            }
                            targets.push(iter.next().unwrap());
                        }
                    }
                let target_str = targets.join(" ");
                Ok(Some(CodeTaal::Send {
                    target: target_str,
                    payload,
                }))
            }
            "matmul" => {
                let size_str = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Verwachte grootte na 'matmul'"))?;
                let size: usize = size_str
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Ongeldige grootte: {}", size_str))?;
                Ok(Some(CodeTaal::MatMul {
                    m: size,
                    n: size,
                    k: size,
                }))
            }
            "functie" | "func" | "fn" | "function" => {
                // functie [naam] met [arg1] [arg2] { ... } -> of 'functie [naam] a b {'
                let name = iter.next().ok_or(anyhow::anyhow!("Verwacht functienaam"))?;
                let mut params = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    if t == "met" || t == "," {
                        iter.next();
                        continue;
                    }
                    params.push(iter.next().unwrap());
                }
                let body_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::FunctionDef {
                    name,
                    params,
                    body: body_ast,
                }))
            }
            "geef_terug" | "return" => {
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().unwrap());
                }
                Ok(Some(CodeTaal::Return {
                    value: val_tokens.join(" "),
                }))
            }
            "gooi" => {
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().unwrap());
                }
                Ok(Some(CodeTaal::Throw {
                    message: val_tokens.join(" "),
                }))
            }
            "voeg_toe" => {
                let array_name = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Verwacht array naam na 'voeg_toe'"))?;
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().unwrap());
                }
                let value = val_tokens.join(" ");
                if value.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Verwacht waarde na array naam in 'voeg_toe'"
                    ));
                }
                Ok(Some(CodeTaal::ArrayPush { array_name, value }))
            }
            "verwijder" => {
                let array_name = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Verwacht array naam na 'verwijder'"))?;
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().unwrap());
                }
                let index = val_tokens.join(" ");
                if index.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Verwacht index na array naam in 'verwijder'"
                    ));
                }
                Ok(Some(CodeTaal::ArrayRemove { array_name, index }))
            }
            "roep_aan" => {
                // top-level roep_aan functie arg1 arg2
                let mut call_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    call_tokens.push(iter.next().unwrap());
                }
                let call_str = call_tokens.join(" ");
                let parts: Vec<&str> = call_str.split_whitespace().collect();
                let name = if !parts.is_empty() {
                    parts[0].to_string()
                } else {
                    "".to_string()
                };
                let args = if parts.len() > 1 {
                    parts[1..].iter().map(|s| s.to_string()).collect()
                } else {
                    Vec::new()
                };
                Ok(Some(CodeTaal::FunctionCall { name, args }))
            }
            "}" => {
                // Einde blok, zou niet hier moeten komen tenzij extra }
                Ok(None)
            }
            ";" => {
                // Semicolon is separator, negeren en doorgaan (return None om loop te continueren? Nee, parse_statement returns Option<Stmt>)
                // Als we None returnen, stopt de loop in parse().
                // We moeten recursief de volgende statement pakken of Loop in parse() aanpassen.
                // Beter: parse() loop aanpassen om None te negeren?
                // parse() doet: while iter.peek().is_some() { if let Some(stmt) = parse_statement()? ... }
                // Dus als we Ok(None) returnen, doet hij niks en gaat naar volgende iteratie.
                // MAAR: we moeten wel zorgen dat we de ';' geconsumed hebben (dat is al gebeurd door iter.next()).
                Ok(None)
            }
            "{" => {
                // Genest blok?
                Ok(Some(Self::parse_block(iter)?))
            }
            _ => {
                // Fallback: SysOp / Command pass-through
                // We verzamelen de rest van de regel tot ;
                let mut args = vec![token];
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    args.push(iter.next().unwrap());
                }
                let command = args.join(" ");
                Ok(Some(CodeTaal::SysOp { command }))
            }
        }
    }

    fn parse_block(iter: &mut Peekable<std::vec::IntoIter<String>>) -> Result<CodeTaal> {
        // Verwacht dat huidige token '{' al geconsumed is of dat we er voor staan?
        // In parse_statement kijken we met peek.
        // Als we hier aangeroepen worden vanuit 'zolang', staan we VOOR de '{'.
        let start = iter.next().ok_or(anyhow::anyhow!("Verwacht '{{'"))?;
        if start != "{" {
            return Err(anyhow::anyhow!("Verwacht '{{'"));
        }

        let mut statements = Vec::new();
        while let Some(token) = iter.peek() {
            if token == "}" {
                iter.next(); // Consume '}'
                return Ok(CodeTaal::Block { statements });
            }
            if let Some(stmt) = Self::parse_statement(iter)? {
                statements.push(stmt);
            }
        }
        Err(anyhow::anyhow!(
            "Onverwacht einde bestand, sluitende '}}' mist."
        ))
    }
}

struct Tokenizer;
impl Tokenizer {
    fn tokenize(input: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        // Remove comment lines first
        let clean_input: String = input
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.starts_with("//"))
            .collect::<Vec<&str>>()
            .join("\n");

        let mut current = String::new();
        let mut in_quote = false;

        let chars: Vec<char> = clean_input.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            match c {
                '"' => {
                    in_quote = !in_quote;
                    current.push(c);
                }
                '{' | '}' | ';' => {
                    if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(current.trim().to_string());
                            current.clear();
                        }
                        tokens.push(c.to_string());
                    } else {
                        current.push(c);
                    }
                }
                ' ' | '\t' | '\n' | '\r' => {
                    if in_quote {
                        current.push(c);
                    } else if !current.trim().is_empty() {
                        tokens.push(current.trim().to_string());
                        current.clear();
                    }
                }
                _ => current.push(c),
            }
            i += 1;
        }
        if !current.trim().is_empty() {
            tokens.push(current.trim().to_string());
        }
        tokens
    }
}
