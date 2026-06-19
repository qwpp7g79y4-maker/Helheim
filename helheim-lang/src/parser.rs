use crate::ast::{CodeTaal, GpuKernelDef, KernelAttribute, GpuParam, GpuType, Precision, GpuOperation, LiteralValue};
use anyhow::Result;
use std::iter::Peekable;

/// De Helheim Parser: Zet 'Helheim' (Naturel) om in Abstracte Logica (AST).
pub struct HelParser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Ident(String),
    Number { raw: String, is_float: bool },
    Op(char),
    LBrace,
    RBrace,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Eof,
    Legacy(String),
}

impl Default for TokenKind {
    fn default() -> Self { TokenKind::Legacy(String::new()) }
}

#[derive(Debug, Clone, Default)]
pub struct Token {
    pub kind: TokenKind,
    pub value: String,
    pub line: usize,
    pub column: usize,
}

impl PartialEq<&str> for Token {
    fn eq(&self, other: &&str) -> bool {
        &self.value == *other
    }
}

impl PartialEq<str> for Token {
    fn eq(&self, other: &str) -> bool {
        self.value == other
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl HelParser {
    pub fn format_parse_error(input: &str, token: &Token, msg: &str) -> anyhow::Error {
        let lines: Vec<&str> = input.lines().collect();
        let line_content = if token.line > 0 && token.line <= lines.len() {
            lines[token.line - 1]
        } else {
            ""
        };
        
        let mut marker = String::new();
        if token.column > 1 {
            for _ in 1..token.column {
                marker.push(' ');
            }
        }
        marker.push_str("^--- ");
        marker.push_str(msg);

        anyhow::anyhow!(
            "Syntax Fout [Regel {}, Kolom {}]: {}\n  |\n{} | {}\n  | {}",
            token.line,
            token.column,
            msg,
            token.line,
            line_content,
            marker
        )
    }

    pub fn parse(input: &str) -> Result<Vec<CodeTaal>> {
        let tokens = Self::tokenize(input);
        let mut iter = tokens.into_iter().peekable();
        let mut ast = Vec::new();

        while iter.peek().is_some() {
            // Inject line/col marker
            if let Some(tok) = iter.peek() {
                ast.push(CodeTaal::LocationMarker { line: tok.line, col: tok.column });
            }
            if let Some(stmt) = Self::parse_statement(input, &mut iter)? {
                ast.push(stmt);
            }
        }
        Ok(ast)
    }

    fn parse_statement(
        input: &str,
        iter: &mut Peekable<std::vec::IntoIter<Token>>,
    ) -> Result<Option<CodeTaal>> {
        let token = match iter.next() {
            Some(t) => t,
            None => return Ok(None),
        };

        match token.value.as_str() {
            "pub" | "publiek" => {
                let stmt_opt = Self::parse_statement(input, iter)?;
                if let Some(mut stmt) = stmt_opt {
                    match &mut stmt {
                        CodeTaal::FunctionDef { is_pub, .. } => *is_pub = true,
                        // Could add VarDef here if needed
                        _ => {}
                    }
                    Ok(Some(stmt))
                } else {
                    Ok(None)
                }
            }
            "gebruik" | "use" | "import" => {
                let path = iter.next().ok_or(anyhow::anyhow!(
                    "Verwacht een bestandsnaam na 'gebruik' of 'use'"
                ))?;
                // Remove semicolon if attached (tokenizer splits them unless quoted, but just in case)
                let clean_path = path.value.trim_matches('"').trim_end_matches(';').to_string();

                // Optionele puntkomma consumeren
                if let Some(next_tok) = iter.peek() {
                    if next_tok == ";" {
                        iter.next();
                    }
                }

                let mut alias = None;
                if let Some(t) = iter.peek() {
                    if t.value == "als" || t.value == "as" {
                        iter.next(); // consume 'als'
                        let alias_tok = iter.next().ok_or(anyhow::anyhow!("Verwacht module naam na 'als'"))?;
                        alias = Some(alias_tok.value.clone());

                        // Optionele puntkomma consumeren
                        if let Some(next_tok) = iter.peek() {
                            if next_tok == ";" {
                                iter.next();
                            }
                        }
                    }
                }

                Ok(Some(CodeTaal::Gebruik { path: clean_path, module_naam: alias }))
            }
            "zet" | "let" | "set" => {
                // zet [naam] = [waarde]
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
                        &format!("Syntax fout: verwachte '=', gevonden '{}'", eq.value)
                    ));
                }

                // Verbeterde value parser: Lees alles tot ';' of ongebalanceerde '}'
                // Stop vóór een nieuw top-level statement als we geen open { hebben.
                // Dit is de key fix voor "diepe returns in functies".
                let mut val_tokens = Vec::new();
                let mut brace_count = 0;
                while let Some(t) = iter.peek() {
                    if t == ";" {
                        break;
                    }

                    if brace_count == 0 && matches!(
                        t.value.as_str(),
                        "zolang" | "while" | "repeat" |
                        "als" | "if" |
                        "retourneer" | "geef_terug" | "return" |
                        "zet" | "let" | "set" |
                        "functie" | "fn" | "func" |
                        "voor" | "for" |
                        "probeer" | "try" |
                        "schrijf" | "print" | "log" |
                        "tcp_luister" | "tcp_listen" |
                        "tcp_stuur" | "tcp_send"
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
                    val_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                
                let expr = if val_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwachte waarde voor '{}'", token.line, name));
                } else if val_tokens.len() == 1 {
                    let v = &val_tokens[0].value;
                    if v.parse::<i64>().is_ok() {
                        Box::new(CodeTaal::Literal(LiteralValue::Int(v.parse().unwrap())))
                    } else if v.parse::<f64>().is_ok() {
                        Box::new(CodeTaal::Literal(LiteralValue::Float(v.parse().unwrap())))
                    } else if v.starts_with("b\"") {
                        let inner = v.trim_start_matches('b').trim_matches('"').to_string();
                        Box::new(CodeTaal::Literal(LiteralValue::Bytes(inner.into_bytes())))
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

                // Optioneel ; consumeren
                if let Some(next) = iter.peek() {
                    if next == ";" {
                        iter.next();
                    }
                }

                Ok(Some(CodeTaal::VarDef { name: name.value.clone(), value: expr }))
            }
            "zolang" | "while" | "repeat" => {
                let mut condition_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "dan" || t == "then" || t == "do" {
                        iter.next();
                        break;
                    }
                    if t == "{" {
                        break;
                    }
                    condition_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                
                let cond_ast = if condition_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwachte conditie voor 'zolang'"));
                } else {
                    let mut expr_iter = condition_tokens.into_iter().peekable();
                    Box::new(Self::parse_expression(input, &mut expr_iter, 0)?)
                };

                // Parse Block
                let body_ast = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::Loop {
                    condition: cond_ast,
                    body: body_ast,
                }))
            }
            "voor" | "for" => {
                // voor elke [item] in [LIJST] { ... }
                let elke = iter.next().unwrap_or_default();
                if elke.value != "elke" && elke.value != "each" {
                    return Err(Self::format_parse_error(input, &elke, "Verwacht 'elke' of 'each' na 'voor'/'for'"));
                }

                let iterator = iter
                    .next()
                    .ok_or(Self::format_parse_error(input, &token, "Verwacht variabele na 'voor elke'"))?;

                let in_kw = iter.next().unwrap_or_default();
                if in_kw.value != "in" {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwacht 'in' na '{}'", token.line, iterator));
                }

                let mut iter_parts = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    iter_parts.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                
                let iterable_ast = if iter_parts.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwachte expressie voor iterable in 'voor'"));
                } else {
                    let mut expr_iter = iter_parts.into_iter().peekable();
                    Box::new(Self::parse_expression(input, &mut expr_iter, 0)?)
                };

                let body_ast = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::ForEach {
                    iterator: iterator.value.clone(),
                    iterable: iterable_ast,
                    body: body_ast,
                }))
            }
            "als" | "if" => {
                let mut condition_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "dan" || t == "then" {
                        iter.next();
                        break;
                    } // Consume 'dan' or 'then'
                    if t == "{" {
                        break;
                    } // Fallback if 'dan' is missing
                    condition_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                
                let cond_ast = if condition_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwachte conditie voor 'als'"));
                } else {
                    let mut expr_iter = condition_tokens.into_iter().peekable();
                    Box::new(Self::parse_expression(input, &mut expr_iter, 0)?)
                };

                let body_ast = Box::new(Self::parse_block(input, iter)?);

                // Optioneel 'anders'/'else' blok vangen
                let mut else_block = None;
                if let Some(next_token) = iter.peek() {
                    if next_token == "anders" || next_token == "else" {
                        // Consume 'anders'
                        iter.next();
                        else_block = Some(Box::new(Self::parse_block(input, iter)?));
                    }
                }

                Ok(Some(CodeTaal::If {
                    condition: cond_ast,
                    then: body_ast,
                    else_block,
                }))
            }
            "tegelijkertijd" | "concurrent" | "async" => {
                let block_ast = Box::new(Self::parse_block(input, iter)?);
                let statements = if let CodeTaal::Block { statements } = *block_ast {
                    statements
                } else {
                    Vec::new()
                };
                Ok(Some(CodeTaal::Concurrent { statements }))
            }
            "achtergrond" | "daemon" => {
                let block_ast = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::Daemon { body: block_ast }))
            }
            "hel" => {
                let open_brace = iter.next().unwrap_or_default();
                if open_brace.value != "{" {
                    return Err(Self::format_parse_error(input, &token, "Verwacht '{' na 'hel'"));
                }
                
                let mut brace_count = 1;
                let mut raw_code_tokens = Vec::new();
                while let Some(t) = iter.next() {
                    if t == "{" {
                        brace_count += 1;
                    } else if t == "}" {
                        brace_count -= 1;
                        if brace_count == 0 {
                            break;
                        }
                    }
                    raw_code_tokens.push(t.value);
                }
                
                let raw_code = raw_code_tokens.join(" ");
                Ok(Some(CodeTaal::HelBlock { raw_code }))
            }
            "probeer" | "try" => {
                // probeer { ... } vang err { ... }
                let try_ast = Box::new(Self::parse_block(input, iter)?);

                let vang_token = iter.next().unwrap_or_default();
                if vang_token.value != "vang" && vang_token.value != "catch" {
                    return Err(Self::format_parse_error(input, &token, "Verwacht 'vang' of 'catch' na 'probeer'-blok"));
                }

                let mut error_var = None;
                if let Some(t) = iter.peek() {
                    if t != "{" {
                        error_var = Some(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.to_string());
                    }
                }

                let catch_ast = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::TryCatch {
                    try_block: try_ast,
                    catch_block: catch_ast,
                    error_var,
                }))
            }
            "stuur" | "send" => {
                // stuur [bericht] naar [targets...]
                // Dit is complexer met tokens.
                // We reconstrueren de zin en gebruiken de bestaande regex/split logic in CodeTaal::Send?
                // Nee, parser moet het doen.
                let payload = iter.next().unwrap_or_default();
                // Als payload tussen quotes staat, is het 1 token.

                let mut targets = Vec::new();
                if let Some(naar) = iter.next() {
                    if naar == "naar" || naar == "to" {
                        while let Some(t) = iter.peek() {
                            if t == ";" || t == "}" {
                                break;
                            }
                            targets.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                        }
                    }
                }
                let target_str = targets.join(" ");
                Ok(Some(CodeTaal::Send {
                    target: target_str,
                    payload: payload.value.clone(),
                }))
            }
            // --- I/O Standaard Bibliotheek (plan: haal/fetch, schrijf/write, lees/read) ---
            "haal" | "fetch" => {
                // haal <url-expr>   (dynamisch: var, literal, of expr)
                let mut url_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    url_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if url_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwachte URL na 'haal' of 'fetch'"));
                }
                let mut expr_iter = url_tokens.into_iter().peekable();
                let url_expr = Self::parse_expression(input, &mut expr_iter, 0)?;
                Ok(Some(CodeTaal::HttpOp {
                    method: "GET".to_string(),
                    url: Box::new(url_expr),
                }))
            }
            "tcp_verbind" | "tcp_connect" => {
                // New primitives-first: returns a stream handle (ResourceHandle at runtime)
                let mut addr_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    addr_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if addr_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwachte host:port na 'tcp_verbind'"));
                }
                let mut expr_iter = addr_tokens.into_iter().peekable();
                let addr_expr = Self::parse_expression(input, &mut expr_iter, 0)?;
                Ok(Some(CodeTaal::TcpConnect { addr: Box::new(addr_expr) }))
            }
            "tcp_luister" | "tcp_listen" => {
                // New primitives: creates listener handle. Use tcp_accepteer to get streams.
                let mut addr_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    addr_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if addr_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwachte bind addr na 'tcp_luister'"));
                }
                let mut expr_iter = addr_tokens.into_iter().peekable();
                let addr_expr = Self::parse_expression(input, &mut expr_iter, 0)?;
                Ok(Some(CodeTaal::TcpListen { addr: Box::new(addr_expr) }))
            }
            "tcp_stuur" | "tcp_send" => {
                // Primitives-first: tcp_stuur <socket_handle_expr> <data_expr>
                // data_expr can be b"..." (Bytes), string, list, etc.
                // Legacy "data naar host" is no longer used; socket handle is primary.
                let mut all_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    all_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if all_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwacht socket en data na 'tcp_stuur'"));
                }

                // Collect up to two expressions. Support comma separation.
                let mut parts: Vec<Vec<Token>> = vec![vec![]];
                for t in all_tokens {
                    if t.value == "," {
                        parts.push(vec![]);
                    } else {
                        parts.last_mut().unwrap().push(t);
                    }
                }

                let mut socket_expr = None;
                let mut data_expr = None;

                if parts.len() > 1 && !parts[0].is_empty() && !parts[1].is_empty() {
                    let mut it0 = parts[0].clone().into_iter().peekable();
                    socket_expr = Some(Self::parse_expression(input, &mut it0, 0)?);
                    let mut it1 = parts[1].clone().into_iter().peekable();
                    data_expr = Some(Self::parse_expression(input, &mut it1, 0)?);
                } else if parts.len() == 1 && parts[0].len() > 1 {
                    let mut it0 = vec![parts[0][0].clone()].into_iter().peekable();
                    socket_expr = Some(Self::parse_expression(input, &mut it0, 0)?);
                    let mut it1 = parts[0][1..].to_vec().into_iter().peekable();
                    data_expr = Some(Self::parse_expression(input, &mut it1, 0)?);
                }

                let socket = socket_expr.ok_or_else(|| Self::format_parse_error(input, &token, "tcp_stuur vereist socket handle als eerste arg"))?;
                let data = data_expr.ok_or_else(|| Self::format_parse_error(input, &token, "tcp_stuur vereist data als tweede arg (b\"...\" of string of list)"))?;

                Ok(Some(CodeTaal::TcpSend { socket: Box::new(socket), data: Box::new(data) }))
            }
            "tcp_accepteer" | "tcp_accept" => {
                // tcp_accepteer listener_handle   → new stream handle (blocks until connection)
                let mut listener_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    listener_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if listener_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwacht listener handle na 'tcp_accepteer'"));
                }
                let mut expr_iter = listener_tokens.into_iter().peekable();
                let listener_expr = Self::parse_expression(input, &mut expr_iter, 0)?;
                Ok(Some(CodeTaal::TcpAccept { listener: Box::new(listener_expr) }))
            }
            "tcp_ontvang" | "tcp_receive" | "tcp_recv" => {
                // tcp_ontvang socket_handle [max_bytes]
                // Returns bytes (as Bytes literal / HelheimType::Bytes at runtime)
                let mut all_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    all_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if all_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwacht socket handle na 'tcp_ontvang'"));
                }

                // Split on comma or just first as socket, rest as max expr
                let mut parts: Vec<Vec<Token>> = vec![vec![]];
                for t in all_tokens {
                    if t.value == "," {
                        parts.push(vec![]);
                    } else {
                        parts.last_mut().unwrap().push(t);
                    }
                }

                let socket_expr = if !parts[0].is_empty() {
                    let mut it = parts[0].clone().into_iter().peekable();
                    Some(Self::parse_expression(input, &mut it, 0)?)
                } else { None };

                let max_expr = if parts.len() > 1 && !parts[1].is_empty() {
                    let mut it = parts[1].clone().into_iter().peekable();
                    Some(Self::parse_expression(input, &mut it, 0)?)
                } else { None };

                let socket = socket_expr.ok_or_else(|| Self::format_parse_error(input, &token, "tcp_ontvang vereist socket handle"))?;

                Ok(Some(CodeTaal::TcpReceive {
                    socket: Box::new(socket),
                    max_bytes: max_expr.map(Box::new),
                }))
            }
            "tcp_sluit" | "tcp_close" => {
                let mut socket_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t.value == ";" || t.value == "}" {
                        break;
                    }
                    socket_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if socket_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwacht socket handle na 'tcp_sluit'"));
                }
                let mut expr_iter = socket_tokens.into_iter().peekable();
                let socket_expr = Self::parse_expression(input, &mut expr_iter, 0)?;
                Ok(Some(CodeTaal::TcpClose { socket: Box::new(socket_expr) }))
            }

            // === ACTOR PRIMITIVES (Vraag 2) ===
            "spawn" | "start_actor" | "ontkiem" | "creëer" => {
                // spawn [name] { body }
                let mut name = None;
                let next = iter.peek();
                if let Some(t) = next {
                    if !t.value.starts_with('{') && !t.value.contains("=>") {
                        // treat as name if not immediately a block
                        let name_tok = iter.next().unwrap();
                        name = Some(name_tok.value.clone());
                    }
                }
                let body = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::Spawn { name, body }))
            }
            "stuur_bericht" | "send_message" => {
                // stuur_bericht <target> <message>
                let target = Self::parse_expression(input, iter, 0)?;
                let message = Self::parse_expression(input, iter, 0)?;
                Ok(Some(CodeTaal::SendMessage {
                    target: Box::new(target),
                    message: Box::new(message),
                }))
            }
            "ontvang" | "receive" | "recv" => {
                // ontvang <var> [timeout] { body }
                let var_tok = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht variabele naam na 'ontvang'"))?;
                let var = var_tok.value.clone();

                let mut timeout = None;
                // peek for timeout expression before {
                if let Some(t) = iter.peek() {
                    if t.value != "{" && t.value != ";" {
                        let to = Self::parse_expression(input, iter, 0)?;
                        timeout = Some(Box::new(to));
                    }
                }

                let body = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::Receive { var, timeout, body }))
            }

            // === ALGEBRAIC EFFECTS (Vraag 6) ===
            "effect" => {
                let name = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht effect naam"))?.value;
                let mut operations = Vec::new();
                let start_brace = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '{' na effect naam"))?;
                if start_brace.value != "{" {
                    return Err(Self::format_parse_error(input, &start_brace, "Verwacht '{'"));
                }
                while let Some(t) = iter.peek() {
                    if t.value == "}" {
                        iter.next();
                        break;
                    }
                    if t.value != "," {
                        operations.push(t.value.clone());
                    }
                    iter.next();
                }
                Ok(Some(CodeTaal::EffectDef { name, operations }))
            }
            "perform" | "voer_uit_effect" => {
                let effect_token = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht effect naam"))?.value;
                let effect;
                let operation;
                if effect_token.contains('.') {
                    let parts: Vec<&str> = effect_token.split('.').collect();
                    effect = parts[0].to_string();
                    operation = parts[1].to_string();
                } else {
                    effect = effect_token;
                    let dot = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '.' na effect naam"))?;
                    if dot.value != "." {
                        return Err(Self::format_parse_error(input, &dot, "Verwacht '.'"));
                    }
                    operation = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht operatie naam"))?.value;
                }
                // parse optional args: perform Tcp.send(arg1, arg2) or perform Tcp.send arg1
                let mut args = Vec::new();
                if let Some(t) = iter.peek() {
                    if t.value == "(" {
                        iter.next(); // consume '('
                        while let Some(t2) = iter.peek() {
                            if t2.value == ")" {
                                iter.next();
                                break;
                            }
                            args.push(Self::parse_expression(input, iter, 0)?);
                            if let Some(c) = iter.peek() {
                                if c.value == "," {
                                    iter.next();
                                }
                            }
                        }
                    } else if t.value != ";" && t.value != "}" {
                        // single arg without parens
                        args.push(Self::parse_expression(input, iter, 0)?);
                    }
                }
                Ok(Some(CodeTaal::Perform { effect, operation, args }))
            }
            "handle" | "handel_af" => {
                let effect = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht effect naam"))?.value;
                
                let mut handlers = Vec::new();
                let start_brace = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '{' na effect naam"))?;
                if start_brace.value != "{" {
                    return Err(Self::format_parse_error(input, &start_brace, "Verwacht '{' voor handlers"));
                }
                while let Some(t) = iter.peek() {
                    if t.value == "}" {
                        iter.next();
                        break;
                    }
                    let op_name = iter.next().unwrap().value;
                    let arrow = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '=>'"))?;
                    if arrow.value != "=>" {
                        return Err(Self::format_parse_error(input, &arrow, "Verwacht '=>'"));
                    }
                    let handler_body = Self::parse_block(input, iter)?;
                    handlers.push((op_name, Box::new(handler_body)));
                    if let Some(c) = iter.peek() {
                        if c.value == "," {
                            iter.next();
                        }
                    }
                }
                
                let in_kw = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht 'in' na handlers"))?;
                if in_kw.value != "in" {
                    return Err(Self::format_parse_error(input, &in_kw, "Verwacht 'in'"));
                }
                let body = Self::parse_block(input, iter)?;
                
                Ok(Some(CodeTaal::Handle { effect, handlers, body: Box::new(body) }))
            }
            "resume" | "hervat" => {
                let continuation = Self::parse_expression(input, iter, 0)?;
                let mut value = CodeTaal::Literal(LiteralValue::Void); // Assume Void is not defined, use Int(0) or similar, actually we just parse expression. Wait, I should add LiteralValue::Void? No, I'll use None or String? Let's check LiteralValue.
                // For now, if there is a comma, parse value
                if let Some(c) = iter.peek() {
                    if c.value == "," {
                        iter.next(); // consume ','
                        value = Self::parse_expression(input, iter, 0)?;
                    }
                }
                Ok(Some(CodeTaal::Resume { continuation: Box::new(continuation), value: Box::new(value) }))
            }

            // [W·AG·AF] C1 Review: InlineAssembly met in/out/clobber
            // === INLINE PTX/ASM (Vraag 1) ===
            "ptx" | "asm" | "inline_asm" | "ruwe_code" | "machinecode" => {
                // ptx { raw ptx or asm source }
                // Optional: inputs like ptx { ... } in (a=expr, b=expr) out (c, d)
                // For simplicity in first version: ptx { ... } with implicit bindings via context
                let mut target = token.value.clone();
                let mut inputs = Vec::new();
                let mut outputs = Vec::new();
                let mut clobbers = Vec::new();

                // If the next token is an identifier and NOT `{`, `in`, `out`, `clobber`, it's the target
                if let Some(t) = iter.peek() {
                    let v = t.value.as_str();
                    if v != "{" && v != "in" && v != "invoer" && v != "out" && v != "uitvoer" && v != "clobber" {
                        target = iter.next().unwrap().value.clone();
                    }
                }

                while let Some(t) = iter.peek() {
                    if t.value == "{" {
                        break;
                    }
                    if t.value == "in" || t.value == "invoer" {
                        iter.next(); // consume 'in'
                        let open = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '(' na 'in'"))?;
                        if open.value != "(" { return Err(Self::format_parse_error(input, &open, "Verwacht '('")); }
                        while let Some(arg) = iter.peek() {
                            if arg.value == ")" {
                                iter.next();
                                break;
                            }
                            if arg.value == "," {
                                iter.next();
                                continue;
                            }
                            let name = iter.next().unwrap().value.clone();
                            let eq = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '=' na in-variabele"))?;
                            if eq.value != "=" { return Err(Self::format_parse_error(input, &eq, "Verwacht '='")); }
                            let expr = Self::parse_expression(input, iter, 0)?;
                            inputs.push((name, Box::new(expr)));
                        }
                    } else if t.value == "out" || t.value == "uitvoer" {
                        iter.next(); // consume 'out'
                        let open = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '(' na 'out'"))?;
                        if open.value != "(" { return Err(Self::format_parse_error(input, &open, "Verwacht '('")); }
                        while let Some(arg) = iter.peek() {
                            if arg.value == ")" {
                                iter.next();
                                break;
                            }
                            if arg.value == "," {
                                iter.next();
                                continue;
                            }
                            outputs.push(iter.next().unwrap().value.clone());
                        }
                    } else if t.value == "clobber" {
                        iter.next(); // consume 'clobber'
                        let open = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '(' na 'clobber'"))?;
                        if open.value != "(" { return Err(Self::format_parse_error(input, &open, "Verwacht '('")); }
                        while let Some(arg) = iter.peek() {
                            if arg.value == ")" {
                                iter.next();
                                break;
                            }
                            if arg.value == "," {
                                iter.next();
                                continue;
                            }
                            clobbers.push(iter.next().unwrap().value.clone().trim_matches('"').to_string());
                        }
                    } else {
                        let err_t = t.clone();
                        return Err(Self::format_parse_error(input, &err_t, "Onverwacht token voor inline assembly block"));
                    }
                }

                let mut code_tokens = Vec::new();
                let start = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '{' na 'ptx'"))?;
                if start.value != "{" {
                    return Err(Self::format_parse_error(input, &start, "Verwacht '{' voor inline assembly block"));
                }

                let mut brace_depth = 1;
                while let Some(t) = iter.next() {
                    if t.value == "{" {
                        brace_depth += 1;
                    } else if t.value == "}" {
                        brace_depth -= 1;
                        if brace_depth == 0 {
                            break;
                        }
                    }
                    code_tokens.push(t.value);
                }

                let code = code_tokens.join(" ");

                let mut fallback = None;
                if let Some(t) = iter.peek() {
                    if t.value == "fallback" || t.value == "terugval" {
                        iter.next(); // consume fallback
                        let fallback_block = Self::parse_block(input, iter)?;
                        fallback = Some(Box::new(fallback_block));
                    }
                }

                Ok(Some(CodeTaal::InlineAssembly {
                    target,
                    code,
                    inputs,
                    outputs,
                    clobbers,
                    fallback,
                }))
            }

            "lees" | "read" => {
                // lees <path-expr>   (dynamisch)
                let mut path_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    path_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if path_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwacht pad na 'lees' of 'read'"));
                }
                let mut expr_iter = path_tokens.into_iter().peekable();
                let path_expr = Self::parse_expression(input, &mut expr_iter, 0)?;
                Ok(Some(CodeTaal::FileOp {
                    action: "read".to_string(),
                    path: Box::new(path_expr),
                    content: None,
                }))
            }
            "schrijf" | "write" => {
                // schrijf <path-expr> <content-expr>
                // of schrijf <content> naar <path> (compat met oude parser)
                let mut all_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    all_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                if all_tokens.is_empty() {
                    return Err(Self::format_parse_error(input, &token, "Verwacht pad en inhoud na 'schrijf' of 'write'"));
                }
                let mut path_expr: Option<CodeTaal> = None;
                let mut content_expr: Option<CodeTaal> = None;
                if let Some(pos) = all_tokens.iter().position(|t| t.value == "naar" || t.value == "to") {
                    let cont_toks: Vec<_> = all_tokens[..pos].to_vec();
                    let path_toks: Vec<_> = all_tokens[pos + 1..].to_vec();
                    if !cont_toks.is_empty() {
                        let mut it = cont_toks.into_iter().peekable();
                        content_expr = Some(Self::parse_expression(input, &mut it, 0)?);
                    }
                    if !path_toks.is_empty() {
                        let mut it = path_toks.into_iter().peekable();
                        path_expr = Some(Self::parse_expression(input, &mut it, 0)?);
                    }
                } else if all_tokens.len() >= 2 {
                    // spec-stijl: schrijf pad inhoud
                    let mut pit = vec![all_tokens[0].clone()].into_iter().peekable();
                    path_expr = Some(Self::parse_expression(input, &mut pit, 0)?);
                    let mut cit = all_tokens[1..].to_vec().into_iter().peekable();
                    content_expr = Some(Self::parse_expression(input, &mut cit, 0)?);
                } else {
                    let mut pit = all_tokens.into_iter().peekable();
                    path_expr = Some(Self::parse_expression(input, &mut pit, 0)?);
                }
                let path = path_expr.ok_or_else(|| Self::format_parse_error(input, &token, "schrijf mist pad"))?;
                Ok(Some(CodeTaal::FileOp {
                    action: "write".to_string(),
                    path: Box::new(path),
                    content: content_expr.map(Box::new),
                }))
            }
            "gedeeld" | "shared" => {
                let _name = iter.next().unwrap_or_default().value;
                // Parse tile_a[16][16] f16;
                // For now, simplify and just consume till ';'
                while let Some(t) = iter.peek() {
                    if t.value == ";" {
                        iter.next();
                        break;
                    }
                    iter.next();
                }
                // Return dummy for now just to prove parsing
                Ok(Some(CodeTaal::GpuOp(GpuOperation::SubgroupSync)))
            }
            "subgroup_sync" => {
                let _open = iter.next(); // '('
                let _close = iter.next(); // ')'
                let _semi = iter.next(); // ';'
                Ok(Some(CodeTaal::GpuOp(GpuOperation::SubgroupSync)))
            }
            "matrix_mma" => {
                // matrix_mma tile_a, tile_b, accum, 16x8x16 f16;
                let a = iter.next().unwrap_or_default().value;
                let _comma = iter.next();
                let b = iter.next().unwrap_or_default().value;
                let _comma2 = iter.next();
                let c = iter.next().unwrap_or_default().value;
                while let Some(t) = iter.peek() {
                    if t.value == ";" {
                        iter.next();
                        break;
                    }
                    iter.next();
                }
                Ok(Some(CodeTaal::GpuOp(GpuOperation::MatrixMultiplyAccumulate {
                    a,
                    b,
                    c,
                    m: 16,
                    n: 8,
                    k: 16,
                    precision: Precision::F16,
                })))
            }
            "matmul" => {
                let size_str = iter
                    .next()
                    .ok_or(Self::format_parse_error(input, &token, "Verwachte grootte na 'matmul'"))?;
                let size: usize = size_str.value
                    .parse()
                    .map_err(|_| Self::format_parse_error(input, &token, &format!("Ongeldige grootte: {}", size_str.value)))?;
                Ok(Some(CodeTaal::MatMul {
                    m: size,
                    n: size,
                    k: size,
                }))
            }
            "gpu_kernel" => {
                let name = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht kernel naam"))?.value.clone();
                let mut attributes = Vec::new();
                
                // Parse attributes #[...]
                while let Some(t) = iter.peek() {
                    if t.value == "#" {
                        iter.next(); // Consume '#'
                        let bracket = iter.next().unwrap_or_default();
                        if bracket.value != "[" {
                            return Err(Self::format_parse_error(input, &token, "Verwacht '[' na '#'"));
                        }
                        let attr_name = iter.next().unwrap_or_default().value;
                        let open_paren = iter.next().unwrap_or_default();
                        if open_paren.value == "(" {
                            let val_str = iter.next().unwrap_or_default().value;
                            let val: u32 = val_str.parse().unwrap_or(32);
                            let _close_paren = iter.next(); // ')'
                            if attr_name == "workgroup" {
                                attributes.push(KernelAttribute::WorkgroupSize(val));
                            } else if attr_name == "subgroup" {
                                attributes.push(KernelAttribute::SubgroupSize(val));
                            }
                        }
                        let _close_bracket = iter.next(); // ']'
                    } else {
                        break;
                    }
                }
                
                // Parse parameters (a: Tensor<f16>, b: ...)
                let mut params = Vec::new();
                let open_paren = iter.next().unwrap_or_default();
                if open_paren.value == "(" {
                    while let Some(t) = iter.peek() {
                        if t.value == ")" {
                            iter.next();
                            break;
                        }
                        let param_name = iter.next().unwrap_or_default().value.clone();
                        let colon = iter.next().unwrap_or_default();
                        if colon.value == ":" {
                            // Read type until ',' or ')'
                            let mut ty_tokens = Vec::new();
                            while let Some(t) = iter.peek() {
                                if t.value == "," || t.value == ")" {
                                    break;
                                }
                                ty_tokens.push(iter.next().unwrap_or_default().value);
                            }
                            let precision = Precision::F16; // default
                            let ty = GpuType::Tensor(precision);
                            params.push(GpuParam {
                                name: param_name,
                                ty,
                            });
                        }
                        let comma = iter.peek().map(|t| t.value.clone()).unwrap_or_default();
                        if comma == "," {
                            iter.next();
                        }
                    }
                }
                
                let body = Self::parse_block(input, iter)?;
                Ok(Some(CodeTaal::GpuKernel(GpuKernelDef {
                    name,
                    attributes,
                    params,
                    body: Box::new(body),
                })))
            }
            "functie" | "func" | "fn" | "function" => {
                // functie [naam] met [arg1] [arg2] { ... } -> of 'functie [naam] a b {'
                let name_tok = iter.next().ok_or(Self::format_parse_error(input, &token, "Verwacht functienaam"))?;
                let mut name = name_tok.value.clone();
                // Check if it's a namespaced function (e.g. http::get)
                while let Some(t) = iter.peek() {
                    if t == ":" {
                        iter.next();
                        if let Some(t2) = iter.peek() {
                            if t2 == ":" {
                                iter.next(); // Consume second colon
                                if let Some(t3) = iter.next() {
                                    name.push_str("::");
                                    name.push_str(&t3.value);
                                    continue;
                                }
                            }
                        }
                    }
                    break;
                }
                let mut params = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    if t == "met" || t == "with" || t == "," || t == "(" || t == ")" {
                        iter.next();
                        continue;
                    }
                    params.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                }
                let body_ast = Box::new(Self::parse_block(input, iter)?);
                Ok(Some(CodeTaal::FunctionDef {
                    name: name.clone(),
                    is_pub: false,
                    params,
                    body: body_ast,
                }))
            }
            "geef_terug" | "retourneer" | "return" => {
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?);
                }
                let value = if val_tokens.is_empty() {
                    None
                } else {
                    let mut expr_iter = val_tokens.into_iter().peekable();
                    Some(Box::new(Self::parse_expression(input, &mut expr_iter, 0)?))
                };
                Ok(Some(CodeTaal::Return { value }))
            }
            "model" => {
                let name = iter.next().unwrap_or_default().to_string();
                let next_token = iter.next().unwrap_or_default();
                if next_token.value != "{" {
                    return Err(Self::format_parse_error(input, &token, "Verwacht '{{' na model naam"));
                }
                let mut fields = Vec::new();
                while let Some(t) = iter.next() {
                    if t.value == "}" {
                        break;
                    }
                    let clean_field = t.value.trim_matches(',').to_string();
                    if !clean_field.is_empty() {
                        fields.push(clean_field);
                    }
                }
                Ok(Some(CodeTaal::ModelDef { name, fields }))
            }
            "gooi" | "throw" => {
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                }
                Ok(Some(CodeTaal::Throw {
                    message: val_tokens.join(" "),
                }))
            }
            "voeg_toe" | "append" => {
                let array_name = iter
                    .next()
                    .ok_or(Self::format_parse_error(input, &token, "Verwacht array naam na 'voeg_toe'"))?;
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                }
                let value = val_tokens.join(" ");
                if value.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Verwacht waarde na array naam in 'voeg_toe'"
                    ));
                }
                Ok(Some(CodeTaal::ArrayPush { array_name: array_name.value.clone(), value }))
            }
            "verwijder" | "remove" => {
                let array_name = iter
                    .next()
                    .ok_or(Self::format_parse_error(input, &token, "Verwacht array naam na 'verwijder'"))?;
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                }
                let index = val_tokens.join(" ");
                if index.is_empty() {
                    return Err(anyhow::anyhow!(
                        "Verwacht index na array naam in 'verwijder'"
                    ));
                }
                Ok(Some(CodeTaal::ArrayRemove { array_name: array_name.value.clone(), index }))
            }
            "roep_aan" | "invoke" | "call" => {
                // roep_aan naam arg1 arg2  (args now full expressions for general CodeTaal)
                let name_tok = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht functienaam na 'roep_aan'"))?;
                let name = name_tok.value.clone();
                let mut args = Vec::new();
                                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    let arg = Self::parse_expression(input, iter, 0)?;
                    args.push(arg);
                    if let Some(c) = iter.peek() {
                        if c == "," { iter.next(); }
                    }
                }
                Ok(Some(CodeTaal::FunctionCall { name, args }))            }
            "rune" => {
                let mut rune_tokens = vec!["rune".to_string()];
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    rune_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                }
                let command = rune_tokens.join(" ");
                Ok(Some(CodeTaal::RuneOp { command }))
            }
            "druk_af" | "print" | "log" => {
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?.value);
                }
                Ok(Some(CodeTaal::Print {
                    message: val_tokens.join(" "),
                }))
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
                // Genest blok? De '{' is al geconsumeerd door parse_statement!
                let mut statements = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "}" {
                        iter.next(); // Consume '}'
                        return Ok(Some(CodeTaal::Block { statements }));
                    }
                    if let Some(stmt) = Self::parse_statement(input, iter)? {
                        statements.push(stmt);
                    }
                }
                Err(anyhow::anyhow!("Fout: Onverwacht einde bestand, sluitende '}}' mist."))
            }
            _ => {
                // Fallback: SysOp / Command pass-through
                let mut command = token.value.clone();
                let mut last_col = token.column + token.value.len();
                while let Some(t) = iter.peek() {
                    if t.value == ";" || t.value == "}" {
                        break;
                    }
                    let next_tok = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Onverwacht einde van het script"))?;
                    
                    if next_tok.column > last_col {
                        for _ in 0..(next_tok.column - last_col) {
                            command.push(' ');
                        }
                    }
                    command.push_str(&next_tok.value);
                    last_col = next_tok.column + next_tok.value.len();
                }
                Ok(Some(CodeTaal::SysOp { command }))
            }
        }
    }

    fn parse_block(input: &str, iter: &mut Peekable<std::vec::IntoIter<Token>>) -> Result<CodeTaal> {
        // Verwacht dat huidige token '{' al geconsumed is of dat we er voor staan?
        // In parse_statement kijken we met peek.
        // Als we hier aangeroepen worden vanuit 'zolang', staan we VOOR de '{'.
        let start = iter.next().ok_or(anyhow::anyhow!("Fout: Verwacht '{{'"))?;
        if start != "{" {
            return Err(Self::format_parse_error(input, &start, "Verwacht '{{'"));
        }

        let mut statements = Vec::new();
        while let Some(token) = iter.peek() {
            if token == "}" {
                iter.next(); // Consume '}'
                return Ok(CodeTaal::Block { statements });
            }
            if let Some(stmt) = Self::parse_statement(input, iter)? {
                statements.push(stmt);
            }
        }
        Err(anyhow::anyhow!(
            "Fout: Onverwacht einde bestand, sluitende '}}' mist."
        ))
    }
}

impl HelParser {
    fn build_token(value: String, line: usize, column: usize) -> Token {
        let kind = if value == "{" { TokenKind::LBrace }
        else if value == "}" { TokenKind::RBrace }
        else if value == "(" { TokenKind::LParen }
        else if value == ")" { TokenKind::RParen }
        else if value == "," { TokenKind::Comma }
        else if value == ";" { TokenKind::Semicolon }
        else if value.len() == 1 && "+-*/=><!&|^%".contains(&value) { TokenKind::Op(value.chars().next().unwrap()) }
        else if value.parse::<f64>().is_ok() { TokenKind::Number { raw: value.clone(), is_float: value.contains('.') } }
        else { TokenKind::Ident(value.clone()) };
        Token { kind, value, line, column }
    }

    pub fn tokenize(input: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_quote = false;
        let mut in_comment = false;
        let mut escape_next = false;
        let mut line_number = 1;
        let mut column_number = 1;
        let mut token_start_col = 1;
        
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            let c = chars[i];
            
            if in_comment {
                if c == '\n' {
                    in_comment = false;
                    line_number += 1;
                    column_number = 1;
                } else {
                    column_number += 1;
                }
                i += 1;
                continue;
            }
            
            if escape_next {
                if current.is_empty() { token_start_col = column_number; }
                if c == 'n' {
                    current.push('\n');
                } else if c == 'r' {
                    current.push('\r');
                } else if c == 't' {
                    current.push('\t');
                } else {
                    current.push(c);
                }
                escape_next = false;
                column_number += 1;
                i += 1;
                continue;
            }
            
            match c {
                '\\' => {
                    if current.is_empty() { token_start_col = column_number; }
                    escape_next = true;
                    column_number += 1;
                }
                '"' => {
                    if current.is_empty() { token_start_col = column_number; }
                    in_quote = !in_quote;
                    current.push(c);
                    column_number += 1;
                }
                '/' => {
                    if !in_quote && i + 1 < chars.len() && chars[i + 1] == '/' {
                        in_comment = true;
                        column_number += 2;
                        i += 1; // Skip the second '/'
                    } else if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                            current.clear();
                        }
                        tokens.push(Self::build_token(c.to_string(), line_number, column_number ));
                        column_number += 1;
                    } else {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                        column_number += 1;
                    }
                }
                '+' | '-' | '*' | '%' | '&' | '|' | '^' => {
                    if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                            current.clear();
                        }
                        if i + 1 < chars.len() && chars[i + 1] == c && (c == '&' || c == '|') {
                            tokens.push(Self::build_token(format!("{}{}", c, c), line_number, column_number ));
                            column_number += 2;
                            i += 1;
                        } else {
                            tokens.push(Self::build_token(c.to_string(), line_number, column_number ));
                            column_number += 1;
                        }
                    } else {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                        column_number += 1;
                    }
                }
                '=' | '!' | '<' | '>' => {
                    if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                            current.clear();
                        }
                        if i + 1 < chars.len() && chars[i + 1] == '=' {
                            tokens.push(Self::build_token(format!("{}=", c), line_number, column_number ));
                            column_number += 2;
                            i += 1;
                        } else if c == '=' && i + 1 < chars.len() && chars[i + 1] == '>' {
                            tokens.push(Self::build_token(format!("=>"), line_number, column_number ));
                            column_number += 2;
                            i += 1;
                        } else {
                            tokens.push(Self::build_token(c.to_string(), line_number, column_number ));
                            column_number += 1;
                        }
                    } else {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                        column_number += 1;
                    }
                }
                ':' => {
                    if !in_quote {
                        if i + 1 < chars.len() && chars[i + 1] == ':' {
                            if current.is_empty() { token_start_col = column_number; }
                            current.push_str("::");
                            column_number += 2;
                            i += 1;
                        } else {
                            if !current.trim().is_empty() {
                                tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                                current.clear();
                            }
                            tokens.push(Self::build_token(c.to_string(), line_number, column_number ));
                            column_number += 1;
                        }
                    } else {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                        column_number += 1;
                    }
                }
                '{' | '}' | ';' | '(' | ')' | '[' | ']' | ',' | '#' => {
                    if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                            current.clear();
                        }
                        tokens.push(Self::build_token(c.to_string(), line_number, column_number ));
                    } else {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                    }
                    column_number += 1;
                }
                ' ' | '\t' | '\r' => {
                    if in_quote {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                    } else if !current.trim().is_empty() {
                        tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                        current.clear();
                    }
                    column_number += 1;
                }
                '\n' => {
                    if in_quote {
                        if current.is_empty() { token_start_col = column_number; }
                        current.push(c);
                    } else if !current.trim().is_empty() {
                        tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
                        current.clear();
                    }
                    line_number += 1;
                    column_number = 1;
                }
                _ => {
                    if current.is_empty() { token_start_col = column_number; }
                    current.push(c);
                    column_number += 1;
                }
            }
            i += 1;
        }
        
        if !current.trim().is_empty() {
            tokens.push(Self::build_token(current.trim().to_string(), line_number, token_start_col ));
        }
        tokens
    }

    fn parse_expression(input: &str, iter: &mut Peekable<std::vec::IntoIter<Token>>, precedence: u8) -> Result<CodeTaal> {
        // Support I/O as expressions too (e.g. zet x = lees p; zet y = haal url_var)
        if let Some(t) = iter.peek() {
            match t.value.as_str() {
                "nieuw" | "new" => {
                    iter.next(); // consume keyword
                    let model_name_tok = iter.next().ok_or_else(|| anyhow::anyhow!("Verwacht model naam na 'nieuw'"))?;
                    let model_name = model_name_tok.value.clone();
                    let mut args = Vec::new();

                    if let Some(t) = iter.peek() {
                        if t.value == "(" {
                            iter.next(); // consume '('
                            while let Some(arg_tok) = iter.peek() {
                                if arg_tok.value == ")" {
                                    iter.next();
                                    break;
                                }
                                if arg_tok.value == "," {
                                    iter.next();
                                    continue;
                                }
                                args.push(arg_tok.value.clone());
                                iter.next();
                            }
                        } else {
                            // fall back to old way? No, parse args until ; or }
                            while let Some(u) = iter.peek() {
                                if u.value == ";" || u.value == "}" { break; }
                                args.push(iter.next().unwrap().value.clone());
                            }
                        }
                    }
                    return Ok(CodeTaal::ModelInit { model_name, args });
                }
                "perform" | "voer_uit_effect" => {
                    let token = iter.next().unwrap(); // consume keyword
                    let effect_token = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht effect naam"))?.value;
                    let effect;
                    let operation;
                    if effect_token.contains('.') {
                        let parts: Vec<&str> = effect_token.split('.').collect();
                        effect = parts[0].to_string();
                        operation = parts[1].to_string();
                    } else {
                        effect = effect_token;
                        let dot = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht '.' na effect naam"))?;
                        if dot.value != "." {
                            return Err(Self::format_parse_error(input, &dot, "Verwacht '.'"));
                        }
                        operation = iter.next().ok_or_else(|| Self::format_parse_error(input, &token, "Verwacht operatie naam"))?.value;
                    }
                    let mut args = Vec::new();
                    if let Some(t) = iter.peek() {
                        if t.value == "(" {
                            iter.next();
                            while let Some(t2) = iter.peek() {
                                if t2.value == ")" {
                                    iter.next();
                                    break;
                                }
                                args.push(Self::parse_expression(input, iter, 0)?);
                                if let Some(c) = iter.peek() {
                                    if c.value == "," { iter.next(); }
                                }
                            }
                        } else if t.value != ";" && t.value != "}" {
                            args.push(Self::parse_expression(input, iter, 0)?);
                        }
                    }
                    return Ok(CodeTaal::Perform { effect, operation, args });
                }
                "roep_aan" | "invoke" | "call" => {
                    iter.next(); // consume keyword
                    let name_tok = iter.next().ok_or_else(|| anyhow::anyhow!("Verwacht functienaam na 'roep_aan'"))?;
                    let name = name_tok.value.clone();
                    let mut args = Vec::new();
                    while let Some(t) = iter.peek() {
                        if t.value == ";" || t.value == "}" { break; }
                        let arg = Self::parse_expression(input, iter, 0)?;
                        args.push(arg);
                        if let Some(c) = iter.peek() {
                            if c.value == "," { iter.next(); }
                        }
                    }
                    return Ok(CodeTaal::FunctionCall { name, args });
                }
                "haal" | "fetch" => {
                    iter.next(); // consume keyword
                    let mut url_tokens = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        url_tokens.push(iter.next().unwrap());
                    }
                    if url_tokens.is_empty() {
                        return Err(anyhow::anyhow!("Verwachte URL na 'haal' in expressie"));
                    }
                    let mut eit = url_tokens.into_iter().peekable();
                    let url = Self::parse_expression(input, &mut eit, 0)?;
                    return Ok(CodeTaal::HttpOp { method: "GET".to_string(), url: Box::new(url) });
                }
                "lees" | "read" => {
                    iter.next();
                    let mut path_tokens = Vec::new();
                    while let Some(p) = iter.peek() {
                        if p == ";" || p == "}" { break; }
                        path_tokens.push(iter.next().unwrap());
                    }
                    if path_tokens.is_empty() {
                        return Err(anyhow::anyhow!("Verwacht pad na 'lees' in expressie"));
                    }
                    let mut eit = path_tokens.into_iter().peekable();
                    let path = Self::parse_expression(input, &mut eit, 0)?;
                    return Ok(CodeTaal::FileOp { action: "read".to_string(), path: Box::new(path), content: None });
                }
                "schrijf" | "write" => {
                    iter.next();
                    // Reuse the statement logic by collecting and delegating
                    let mut all = Vec::new();
                    while let Some(tt) = iter.peek() {
                        if tt == ";" || tt == "}" { break; }
                        all.push(iter.next().unwrap());
                    }
                    // For simplicity in expr context, require at least path+content
                    if all.len() < 2 {
                        return Err(anyhow::anyhow!("schrijf in expressie verwacht pad + inhoud"));
                    }
                    let mut pit = vec![all[0].clone()].into_iter().peekable();
                    let path = Self::parse_expression(input, &mut pit, 0)?;
                    let mut cit = all[1..].to_vec().into_iter().peekable();
                    let content = Self::parse_expression(input, &mut cit, 0)?;
                    return Ok(CodeTaal::FileOp { action: "write".to_string(), path: Box::new(path), content: Some(Box::new(content)) });
                }
                // TCP primitives as expressions (zet s = tcp_verbind "...")
                "tcp_verbind" | "tcp_connect" => {
                    iter.next();
                    let mut addr_toks = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        addr_toks.push(iter.next().unwrap());
                    }
                    if addr_toks.is_empty() {
                        return Err(anyhow::anyhow!("Verwachte addr na tcp_verbind in expr"));
                    }
                    let mut eit = addr_toks.into_iter().peekable();
                    let addr = Self::parse_expression(input, &mut eit, 0)?;
                    return Ok(CodeTaal::TcpConnect { addr: Box::new(addr) });
                }
                "tcp_luister" | "tcp_listen" => {
                    iter.next();
                    let mut addr_toks = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        addr_toks.push(iter.next().unwrap());
                    }
                    if addr_toks.is_empty() {
                        return Err(anyhow::anyhow!("Verwachte addr na tcp_luister in expr"));
                    }
                    let mut eit = addr_toks.into_iter().peekable();
                    let addr = Self::parse_expression(input, &mut eit, 0)?;
                    return Ok(CodeTaal::TcpListen { addr: Box::new(addr) });
                }
                "tcp_accepteer" | "tcp_accept" => {
                    iter.next();
                    let mut lis_toks = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        lis_toks.push(iter.next().unwrap());
                    }
                    if lis_toks.is_empty() {
                        return Err(anyhow::anyhow!("Verwachte listener na tcp_accepteer in expr"));
                    }
                    let mut eit = lis_toks.into_iter().peekable();
                    let lis = Self::parse_expression(input, &mut eit, 0)?;
                    return Ok(CodeTaal::TcpAccept { listener: Box::new(lis) });
                }
                "tcp_stuur" | "tcp_send" => {
                    iter.next();
                    // Minimal support in expr context: tcp_stuur sock data  (returns unit-ish for now)
                    let mut toks = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        toks.push(iter.next().unwrap());
                    }
                    // For expr we still produce the node; execution will handle side-effect
                    if toks.len() < 2 {
                        return Err(anyhow::anyhow!("tcp_stuur in expr verwacht socket + data"));
                    }
                    let mut sit = vec![toks[0].clone()].into_iter().peekable();
                    let sock = Self::parse_expression(input, &mut sit, 0)?;
                    let mut dit = toks[1..].to_vec().into_iter().peekable();
                    let dat = Self::parse_expression(input, &mut dit, 0)?;
                    return Ok(CodeTaal::TcpSend { socket: Box::new(sock), data: Box::new(dat) });
                }
                "tcp_ontvang" | "tcp_receive" | "tcp_recv" => {
                    iter.next();
                    let mut toks = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        toks.push(iter.next().unwrap());
                    }
                    if toks.is_empty() {
                        return Err(anyhow::anyhow!("tcp_ontvang in expr verwacht socket"));
                    }
                    let mut sit = vec![toks[0].clone()].into_iter().peekable();
                    let sock = Self::parse_expression(input, &mut sit, 0)?;
                    let maxb = if toks.len() > 1 {
                        let mut mit = toks[1..].to_vec().into_iter().peekable();
                        Some(Box::new(Self::parse_expression(input, &mut mit, 0)?))
                    } else { None };
                    return Ok(CodeTaal::TcpReceive { socket: Box::new(sock), max_bytes: maxb });
                }
                "tcp_sluit" | "tcp_close" => {
                    iter.next();
                    let mut toks = Vec::new();
                    while let Some(u) = iter.peek() {
                        if u == ";" || u == "}" { break; }
                        toks.push(iter.next().unwrap());
                    }
                    if toks.is_empty() {
                        return Err(anyhow::anyhow!("tcp_sluit in expr verwacht socket"));
                    }
                    let mut sit = toks.into_iter().peekable();
                    let sock = Self::parse_expression(input, &mut sit, 0)?;
                    return Ok(CodeTaal::TcpClose { socket: Box::new(sock) });
                }
                _ => {}
            }
        }

        // List literals: [waar, onwaar, ...] (1D) and [[..],[..]] (2D matrices)
        if let Some(t) = iter.peek() {
            if t.value == "[" {
                iter.next(); // consume [
                // Peek to see if 2D: next non-comma is another [
                let mut is_2d = false;
                // simple peek for nested structure
                if let Some(next) = iter.peek() {
                    if next.value == "[" {
                        is_2d = true;
                    }
                }
                if is_2d {
                    let mut rows: Vec<Vec<LiteralValue>> = Vec::new();
                    while let Some(tok) = iter.peek() {
                        if tok.value == "]" {
                            iter.next();
                            break;
                        }
                        if tok.value == "," {
                            iter.next();
                            continue;
                        }
                        if tok.value == "[" {
                            // parse sub list (row)
                            iter.next(); // [
                            let mut row = Vec::new();
                            while let Some(stok) = iter.peek() {
                                if stok.value == "]" {
                                    iter.next();
                                    break;
                                }
                                if stok.value == "," {
                                    iter.next();
                                    continue;
                                }
                                let sitem = iter.next().unwrap();
                                let slit = if sitem.value == "waar" || sitem.value == "true" {
                                    LiteralValue::Bool(true)
                                } else if sitem.value == "onwaar" || sitem.value == "false" {
                                    LiteralValue::Bool(false)
                                } else if sitem.value.parse::<i64>().is_ok() {
                                    LiteralValue::Int(sitem.value.parse().unwrap())
                                } else if sitem.value.parse::<f64>().is_ok() {
                                    LiteralValue::Float(sitem.value.parse().unwrap())
                                } else if sitem.value.starts_with("\"") {
                                    if sitem.value.starts_with("b\"") {
                                let inner = sitem.value.trim_start_matches('b').trim_matches('"').to_string();
                                LiteralValue::Bytes(inner.into_bytes())
                            } else {
                                LiteralValue::String(sitem.value.trim_matches('"').to_string())
                            }
                                } else {
                                    return Err(anyhow::anyhow!("Unsupported matrix item: {}", sitem.value));
                                };
                                row.push(slit);
                            }
                            rows.push(row);
                        }
                    }
                    return Ok(CodeTaal::MatrixLiteral { rows });
                } else {
                    // 1D list
                    let mut items = Vec::new();
                    while let Some(tok) = iter.peek() {
                        if tok.value == "]" {
                            iter.next();
                            break;
                        }
                        if tok.value == "," {
                            iter.next();
                            continue;
                        }
                        let item_tok = iter.next().unwrap();
                        let lit = if item_tok.value == "waar" || item_tok.value == "true" {
                            LiteralValue::Bool(true)
                        } else if item_tok.value == "onwaar" || item_tok.value == "false" {
                            LiteralValue::Bool(false)
                        } else if item_tok.value.parse::<i64>().is_ok() {
                            LiteralValue::Int(item_tok.value.parse().unwrap())
                        } else if item_tok.value.parse::<f64>().is_ok() {
                            LiteralValue::Float(item_tok.value.parse().unwrap())
                        } else if item_tok.value.starts_with("\"") {
                            if item_tok.value.starts_with("b\"") {
                                let inner = item_tok.value.trim_start_matches('b').trim_matches('"').to_string();
                                LiteralValue::Bytes(inner.into_bytes())
                            } else {
                                LiteralValue::String(item_tok.value.trim_matches('"').to_string())
                            }
                        } else {
                            return Err(anyhow::anyhow!("Unsupported list item in literal: {}", item_tok.value));
                        };
                        items.push(lit);
                    }
                    return Ok(CodeTaal::ListLiteral { items });
                }
            }
        }

        // Function call in expression context: naam(arg1, arg2, ...)
        if let Some(t) = iter.peek() {
            if !t.value.starts_with("\"") && t.value != "[" && t.value != "{" && !t.value.parse::<f64>().is_ok() && t.value != "waar" && t.value != "onwaar" {
                // potential id
                let name = t.value.clone();
                // peek next for (
                // but since we haven't consumed, we need to look ahead carefully
                // for simplicity, consume if next is (
                let mut temp = iter.clone();
                temp.next(); // the id
                if let Some(next) = temp.peek() {
                    if next.value == "(" {
                        // it's a call
                        iter.next(); // consume name
                        iter.next(); // consume (
                        let mut args = Vec::new();
                        while let Some(tok) = iter.peek() {
                            if tok.value == ")" {
                                iter.next();
                                break;
                            }
                            if tok.value == "," {
                                iter.next();
                                continue;
                            }
                            let arg = Self::parse_expression(input, iter, 0)?;
                            args.push(arg);
                            if let Some(com) = iter.peek() {
                                if com.value == "," { iter.next(); }
                            }
                        }
                        let call = if name == "popc" || name == "popcount" {
                            if args.len() == 1 {
                                CodeTaal::Op {
                                    left: Box::new(args.into_iter().next().unwrap()),
                                    op: "popc".to_string(),
                                    right: Box::new(CodeTaal::Literal(LiteralValue::Int(0))),
                                }
                            } else {
                                CodeTaal::FunctionCall { name, args }
                            }
                        } else {
                            CodeTaal::FunctionCall { name, args }
                        };
                        
                        let mut left = call;
                        while let Some(t) = iter.peek() {
                            if t.value == ";" || t.value == "}" { break; }
                            let op_prec = Self::get_precedence(&t.value);
                            if op_prec < precedence || op_prec == 0 { break; }
                            let op = iter.next().unwrap().value;
                            let right = Self::parse_expression(input, iter, op_prec + 1)?;
                            left = CodeTaal::Op {
                                left: Box::new(left),
                                op,
                                right: Box::new(right),
                            };
                        }
                        return Ok(left);
                    }
                }
            }
        }

        let mut left = match iter.next() {
            Some(t) => {
                if t.value == "(" {
                    let expr = Self::parse_expression(input, iter, 0)?;
                    if let Some(next) = iter.next() {
                        if next.value != ")" {
                            return Err(anyhow::anyhow!("Verwacht sluitend haakje ')', gevonden '{}'", next.value));
                        }
                    } else {
                        return Err(anyhow::anyhow!("Verwacht sluitend haakje ')'"));
                    }
                    expr
                } else if let TokenKind::Number { raw, is_float } = &t.kind {
                    if *is_float {
                        CodeTaal::Literal(LiteralValue::Float(raw.parse().unwrap_or(0.0)))
                    } else {
                        CodeTaal::Literal(LiteralValue::Int(raw.parse().unwrap_or(0)))
                    }
                } else if t.value.starts_with("b\"") {
                    // Rock-solid byte literal: b"raw bytes here" → Bytes(Vec<u8>)
                    let inner = t.value.trim_start_matches('b').trim_matches('"').to_string();
                    CodeTaal::Literal(LiteralValue::Bytes(inner.into_bytes()))
                } else if t.value.starts_with("\"") {
                    let s = t.value.trim_matches('"').to_string();
                    CodeTaal::Literal(LiteralValue::String(s))
                } else {
                    let mut name = t.value;
                    if let Some(peek) = iter.peek() {
                        if peek.value == "[" {
                            name.push_str(&iter.next().unwrap().value); // "["
                            while let Some(nt) = iter.peek() {
                                let val = nt.value.clone();
                                name.push_str(&val);
                                iter.next();
                                if val == "]" {
                                    break;
                                }
                            }
                        }
                    }
                    CodeTaal::VarGet { name }
                }
            },
            None => return Err(anyhow::anyhow!("Unexpected end of expression")),
        };

        while let Some(t) = iter.peek() {
            if t.value == ";" || t.value == "}" { break; }
            let op_prec = Self::get_precedence(&t.value);
            if op_prec < precedence || op_prec == 0 { break; }

            let op = iter.next().unwrap().value;
            let right = Self::parse_expression(input, iter, op_prec + 1)?;
            left = CodeTaal::Op {
                left: Box::new(left),
                op,
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    fn get_precedence(op: &str) -> u8 {
        match op {
            "||" | "of" => 2,
            "&&" | "en" => 3,
            "==" | "!=" | "<" | ">" | "<=" | ">=" => 5,
            "&" | "|" | "^" => 6,
            "<<" | ">>" => 7,
            "+" | "-" => 10,
            "*" | "/" | "%" => 20,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_diagnostics_line_and_col() {
        let code2 = "zet = 10;";
        let res = HelParser::parse(code2);
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        println!("{}", err_msg);
        assert!(err_msg.contains("Syntax Fout [Regel 1, Kolom 7]: Syntax fout: verwachte '=', gevonden '10'"));
        assert!(err_msg.contains("^---"));
    }

    #[test]
    fn test_parser_diagnostics_missing_brace() {
        let code = "als x == 1 dan\n    druk_af \"hoi\";\nanders\n    druk_af \"doei\";";
        let res = HelParser::parse(code);
        assert!(res.is_err());
        let err_msg = res.unwrap_err().to_string();
        println!("{}", err_msg); assert!(err_msg.contains("Syntax Fout [Regel 2"));
    }
    #[test]
    fn test_logic_operator_ast() {
        let code = "zet is_or = x == 5 || y == 20;";
        let ast = HelParser::parse(code).unwrap();
        println!("{:#?}", ast);
    }

    #[test]
    fn test_daemon_ast() {
        let script = r#"
            achtergrond {
                invoke wacht 0;
                let daemon_klaar = "ja";
                invoke bestand.schrijf "daemon_test.txt" daemon_klaar;
            }
        "#;
        let ast = HelParser::parse(script).unwrap();
        println!("{:#?}", ast);
    }
}
