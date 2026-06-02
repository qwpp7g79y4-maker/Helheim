use crate::ast::{CodeTaal, GpuKernelDef, KernelAttribute, GpuParam, GpuType, Precision, GpuOperation, LiteralValue};
use anyhow::Result;
use std::iter::Peekable;

/// De Helheim Parser: Zet 'Helheim' (Naturel) om in Abstracte Logica (AST).
pub struct HelParser;

#[derive(Debug, Clone, Default)]
pub struct Token {
    pub value: String,
    pub line: usize,
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
    pub fn parse(input: &str) -> Result<Vec<CodeTaal>> {
        let tokens = Self::tokenize(input);
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
        iter: &mut Peekable<std::vec::IntoIter<Token>>,
    ) -> Result<Option<CodeTaal>> {
        let token = match iter.next() {
            Some(t) => t,
            None => return Ok(None),
        };

        match token.value.as_str() {
            "gebruik" | "use" | "import" => {
                let path = iter.next().ok_or(anyhow::anyhow!(
                    "Verwacht een bestandsnaam na 'gebruik' of 'use'"
                ))?;
                // Remove semicolon if attached (tokenizer splits them unless quoted, but just in case)
                let clean_path = path.value.trim_matches('"').trim_end_matches(';').to_string();

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
                    .ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwachte variabele naam na 'zet'"))?;
                let eq = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwachte '=' na variabele"))?;
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
                    val_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                
                let expr = if val_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwachte waarde voor '{}'", token.line, name));
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
                    Box::new(Self::parse_expression(&mut expr_iter, 0)?)
                };

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
                    condition_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                
                let cond_ast = if condition_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwachte conditie voor 'zolang'", token.line));
                } else {
                    let mut expr_iter = condition_tokens.into_iter().peekable();
                    Box::new(Self::parse_expression(&mut expr_iter, 0)?)
                };

                // Parse Block
                let body_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::Loop {
                    condition: cond_ast,
                    body: body_ast,
                }))
            }
            "voor" | "for" => {
                // voor elke [item] in [LIJST] { ... }
                let elke = iter.next().unwrap_or_default();
                if elke.value != "elke" && elke.value != "each" {
                    return Err(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht 'elke' of 'each' na 'voor'/'for'"));
                }

                let iterator = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht variabele na 'voor elke'"))?;

                let in_kw = iter.next().unwrap_or_default();
                if in_kw.value != "in" {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwacht 'in' na '{}'", token.line, iterator));
                }

                // We consume everything till '{' as the iterable string
                let mut iter_parts = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    iter_parts.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
                }
                let iterable = iter_parts.join(" ");

                let body_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::ForEach {
                    iterator: iterator.value.clone(),
                    iterable,
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
                    condition_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                
                let cond_ast = if condition_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwachte conditie voor 'als'", token.line));
                } else {
                    let mut expr_iter = condition_tokens.into_iter().peekable();
                    Box::new(Self::parse_expression(&mut expr_iter, 0)?)
                };

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
            "tegelijkertijd" | "concurrent" | "async" => {
                let block_ast = Box::new(Self::parse_block(iter)?);
                let statements = if let CodeTaal::Block { statements } = *block_ast {
                    statements
                } else {
                    Vec::new()
                };
                Ok(Some(CodeTaal::Concurrent { statements }))
            }
            "achtergrond" | "daemon" => {
                let block_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::Daemon { body: block_ast }))
            }
            "probeer" | "try" => {
                // probeer { ... } vang err { ... }
                let try_ast = Box::new(Self::parse_block(iter)?);

                let vang_token = iter.next().unwrap_or_default();
                if vang_token.value != "vang" && vang_token.value != "catch" {
                    return Err(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht 'vang' of 'catch' na 'probeer'-blok"));
                }

                let mut error_var = None;
                if let Some(t) = iter.peek()
                    && t != "{" {
                        error_var = Some(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.to_string());
                    }

                let catch_ast = Box::new(Self::parse_block(iter)?);
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
                if let Some(naar) = iter.next()
                    && (naar == "naar" || naar == "to") {
                        while let Some(t) = iter.peek() {
                            if t == ";" || t == "}" {
                                break;
                            }
                            targets.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
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
                    url_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                if url_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwachte URL na 'haal' of 'fetch'", token.line));
                }
                let mut expr_iter = url_tokens.into_iter().peekable();
                let url_expr = Self::parse_expression(&mut expr_iter, 0)?;
                Ok(Some(CodeTaal::HttpOp {
                    method: "GET".to_string(),
                    url: Box::new(url_expr),
                }))
            }
            "lees" | "read" => {
                // lees <path-expr>   (dynamisch)
                let mut path_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    path_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                if path_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwacht pad na 'lees' of 'read'", token.line));
                }
                let mut expr_iter = path_tokens.into_iter().peekable();
                let path_expr = Self::parse_expression(&mut expr_iter, 0)?;
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
                    all_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                if all_tokens.is_empty() {
                    return Err(anyhow::anyhow!("Fout op regel {}: Verwacht pad en inhoud na 'schrijf' of 'write'", token.line));
                }
                let mut path_expr: Option<CodeTaal> = None;
                let mut content_expr: Option<CodeTaal> = None;
                if let Some(pos) = all_tokens.iter().position(|t| t.value == "naar" || t.value == "to") {
                    let cont_toks: Vec<_> = all_tokens[..pos].to_vec();
                    let path_toks: Vec<_> = all_tokens[pos + 1..].to_vec();
                    if !cont_toks.is_empty() {
                        let mut it = cont_toks.into_iter().peekable();
                        content_expr = Some(Self::parse_expression(&mut it, 0)?);
                    }
                    if !path_toks.is_empty() {
                        let mut it = path_toks.into_iter().peekable();
                        path_expr = Some(Self::parse_expression(&mut it, 0)?);
                    }
                } else if all_tokens.len() >= 2 {
                    // spec-stijl: schrijf pad inhoud
                    let mut pit = vec![all_tokens[0].clone()].into_iter().peekable();
                    path_expr = Some(Self::parse_expression(&mut pit, 0)?);
                    let mut cit = all_tokens[1..].to_vec().into_iter().peekable();
                    content_expr = Some(Self::parse_expression(&mut cit, 0)?);
                } else {
                    let mut pit = all_tokens.into_iter().peekable();
                    path_expr = Some(Self::parse_expression(&mut pit, 0)?);
                }
                let path = path_expr.ok_or_else(|| anyhow::anyhow!("Fout op regel {}: schrijf mist pad", token.line))?;
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
                    .ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwachte grootte na 'matmul'"))?;
                let size: usize = size_str.value
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Fout op regel {}: Ongeldige grootte: {}", token.line, size_str.value))?;
                Ok(Some(CodeTaal::MatMul {
                    m: size,
                    n: size,
                    k: size,
                }))
            }
            "gpu_kernel" => {
                let name = iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: Verwacht kernel naam", token.line))?.value.clone();
                let mut attributes = Vec::new();
                
                // Parse attributes #[...]
                while let Some(t) = iter.peek() {
                    if t.value == "#" {
                        iter.next(); // Consume '#'
                        let bracket = iter.next().unwrap_or_default();
                        if bracket.value != "[" {
                            return Err(anyhow::anyhow!("Fout: Verwacht '[' na '#'"));
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
                
                let body = Self::parse_block(iter)?;
                Ok(Some(CodeTaal::GpuKernel(GpuKernelDef {
                    name,
                    attributes,
                    params,
                    body: Box::new(body),
                })))
            }
            "functie" | "func" | "fn" | "function" => {
                // functie [naam] met [arg1] [arg2] { ... } -> of 'functie [naam] a b {'
                let name = iter.next().ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht functienaam"))?;
                let mut params = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == "{" {
                        break;
                    }
                    if t == "met" || t == "with" || t == "," {
                        iter.next();
                        continue;
                    }
                    params.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
                }
                let body_ast = Box::new(Self::parse_block(iter)?);
                Ok(Some(CodeTaal::FunctionDef {
                    name: name.value.clone(),
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
                    val_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                let value = if val_tokens.is_empty() {
                    None
                } else {
                    let mut expr_iter = val_tokens.into_iter().peekable();
                    Some(Box::new(Self::parse_expression(&mut expr_iter, 0)?))
                };
                Ok(Some(CodeTaal::Return { value }))
            }
            "model" => {
                let name = iter.next().unwrap_or_default().to_string();
                let next_token = iter.next().unwrap_or_default();
                if next_token.value != "{" {
                    return Err(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht '{{' na model naam"));
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
                    val_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
                }
                Ok(Some(CodeTaal::Throw {
                    message: val_tokens.join(" "),
                }))
            }
            "voeg_toe" | "append" => {
                let array_name = iter
                    .next()
                    .ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht array naam na 'voeg_toe'"))?;
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
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
                    .ok_or(anyhow::anyhow!("Fout op regel {}: {}", token.line, "Verwacht array naam na 'verwijder'"))?;
                let mut val_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    val_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
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
                // top-level roep_aan functie arg1 arg2
                let mut call_tokens = Vec::new();
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    call_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?);
                }
                let name = if !call_tokens.is_empty() {
                    call_tokens[0].value.clone()
                } else {
                    "".to_string()
                };
                let mut args = Vec::new();
                if call_tokens.len() > 1 {
                    for arg_tok in &call_tokens[1..] {
                        let expr = if arg_tok.value.parse::<f64>().is_ok() {
                            CodeTaal::Literal(LiteralValue::Float(arg_tok.value.parse().unwrap()))
                        } else if arg_tok.value.parse::<i64>().is_ok() {
                            CodeTaal::Literal(LiteralValue::Int(arg_tok.value.parse().unwrap()))
                        } else if arg_tok.value.starts_with("\"") {
                            CodeTaal::Literal(LiteralValue::String(arg_tok.value.trim_matches('"').to_string()))
                        } else {
                            CodeTaal::VarGet { name: arg_tok.value.clone() }
                        };
                        args.push(expr);
                    }
                }
                Ok(Some(CodeTaal::FunctionCall { name, args }))
            }
            "rune" => {
                let mut rune_tokens = vec!["rune".to_string()];
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    rune_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
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
                    val_tokens.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
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
                // Genest blok?
                Ok(Some(Self::parse_block(iter)?))
            }
            _ => {
                // Fallback: SysOp / Command pass-through
                // We verzamelen de rest van de regel tot ;
                let mut args = vec![token.value.clone()];
                while let Some(t) = iter.peek() {
                    if t == ";" || t == "}" {
                        break;
                    }
                    args.push(iter.next().ok_or_else(|| anyhow::anyhow!("Fout op regel {}: {}", token.line, "Onverwacht einde van het script"))?.value);
                }
                let command = args.join(" ");
                Ok(Some(CodeTaal::SysOp { command }))
            }
        }
    }

    fn parse_block(iter: &mut Peekable<std::vec::IntoIter<Token>>) -> Result<CodeTaal> {
        // Verwacht dat huidige token '{' al geconsumed is of dat we er voor staan?
        // In parse_statement kijken we met peek.
        // Als we hier aangeroepen worden vanuit 'zolang', staan we VOOR de '{'.
        let start = iter.next().ok_or(anyhow::anyhow!("Fout: Verwacht '{{'"))?;
        if start != "{" {
            return Err(anyhow::anyhow!("Fout op regel {}: Verwacht '{{'", start.line));
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
            "Fout: Onverwacht einde bestand, sluitende '}}' mist."
        ))
    }
}

impl HelParser {
    pub fn tokenize(input: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut current = String::new();
        let mut in_quote = false;
        let mut in_comment = false;
        let mut escape_next = false;
        let mut line_number = 1;
        
        let chars: Vec<char> = input.chars().collect();
        let mut i = 0;
        
        while i < chars.len() {
            let c = chars[i];
            
            if in_comment {
                if c == '\n' {
                    in_comment = false;
                    line_number += 1;
                }
                i += 1;
                continue;
            }
            
            if escape_next {
                current.push(c);
                escape_next = false;
                i += 1;
                continue;
            }
            
            match c {
                '\\' => {
                    escape_next = true;
                    current.push(c);
                }
                '"' => {
                    in_quote = !in_quote;
                    current.push(c);
                }
                '/' if !in_quote && i + 1 < chars.len() && chars[i + 1] == '/' => {
                    in_comment = true;
                    i += 1; // Skip the second '/'
                }
                '=' | '!' | '<' | '>' => {
                    if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(Token { value: current.trim().to_string(), line: line_number });
                            current.clear();
                        }
                        if i + 1 < chars.len() && chars[i + 1] == '=' {
                            tokens.push(Token { value: format!("{}=", c), line: line_number });
                            i += 1;
                        } else {
                            tokens.push(Token { value: c.to_string(), line: line_number });
                        }
                    } else {
                        current.push(c);
                    }
                }
                '{' | '}' | ';' | '(' | ')' | '[' | ']' | ',' | ':' | '#' => {
                    if !in_quote {
                        if !current.trim().is_empty() {
                            tokens.push(Token { value: current.trim().to_string(), line: line_number });
                            current.clear();
                        }
                        tokens.push(Token { value: c.to_string(), line: line_number });
                    } else {
                        current.push(c);
                    }
                }
                ' ' | '\t' | '\r' => {
                    if in_quote {
                        current.push(c);
                    } else if !current.trim().is_empty() {
                        tokens.push(Token { value: current.trim().to_string(), line: line_number });
                        current.clear();
                    }
                }
                '\n' => {
                    if in_quote {
                        current.push(c);
                    } else if !current.trim().is_empty() {
                        tokens.push(Token { value: current.trim().to_string(), line: line_number });
                        current.clear();
                    }
                    line_number += 1;
                }
                _ => {
                    current.push(c);
                }
            }
            i += 1;
        }
        
        if !current.trim().is_empty() {
            tokens.push(Token { value: current.trim().to_string(), line: line_number });
        }
        tokens
    }

    fn parse_expression(iter: &mut Peekable<std::vec::IntoIter<Token>>, precedence: u8) -> Result<CodeTaal> {
        // Support I/O as expressions too (e.g. zet x = lees p; zet y = haal url_var)
        if let Some(t) = iter.peek() {
            match t.value.as_str() {
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
                    let url = Self::parse_expression(&mut eit, 0)?;
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
                    let path = Self::parse_expression(&mut eit, 0)?;
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
                    let path = Self::parse_expression(&mut pit, 0)?;
                    let mut cit = all[1..].to_vec().into_iter().peekable();
                    let content = Self::parse_expression(&mut cit, 0)?;
                    return Ok(CodeTaal::FileOp { action: "write".to_string(), path: Box::new(path), content: Some(Box::new(content)) });
                }
                _ => {}
            }
        }

        // Support list literals for spikes/booleans: [waar, onwaar, ...]
        if let Some(t) = iter.peek() {
            if t.value == "[" {
                iter.next(); // consume [
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
                    // parse simple literal
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
                        LiteralValue::String(item_tok.value.trim_matches('"').to_string())
                    } else {
                        return Err(anyhow::anyhow!("Unsupported list item in literal: {}", item_tok.value));
                    };
                    items.push(lit);
                }
                return Ok(CodeTaal::ListLiteral { items });
            }
        }

        // Support simple function calls for intrinsics like tel_spikes(overlap) or popc(x) for SNN popcount
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
                            let arg = Self::parse_expression(iter, 0)?;
                            args.push(arg);
                            if let Some(com) = iter.peek() {
                                if com.value == "," { iter.next(); }
                            }
                        }
                        if name == "tel_spikes" || name == "popc" || name == "popcount" {
                            // special for SNN popcount, return as Op "popc" for lowering
                            if args.len() == 1 {
                                return Ok(CodeTaal::Op {
                                    left: Box::new(args.into_iter().next().unwrap()),
                                    op: "popc".to_string(),
                                    right: Box::new(CodeTaal::Literal(LiteralValue::Int(0))),
                                });
                            }
                        }
                        return Ok(CodeTaal::FunctionCall { name, args });
                    }
                }
            }
        }

        let mut left = match iter.next() {
            Some(t) => {
                if t.value.parse::<i64>().is_ok() {
                    CodeTaal::Literal(LiteralValue::Int(t.value.parse().unwrap()))
                } else if t.value.parse::<f64>().is_ok() {
                    CodeTaal::Literal(LiteralValue::Float(t.value.parse().unwrap()))
                } else if t.value.starts_with("\"") {
                    let s = t.value.trim_matches('"').to_string();
                    CodeTaal::Literal(LiteralValue::String(s))
                } else {
                    CodeTaal::VarGet { name: t.value }
                }
            },
            None => return Err(anyhow::anyhow!("Unexpected end of expression")),
        };

        while let Some(t) = iter.peek() {
            if t.value == ";" || t.value == "}" { break; }
            let op_prec = Self::get_precedence(&t.value);
            if op_prec < precedence || op_prec == 0 { break; }

            let op = iter.next().unwrap().value;
            let right = Self::parse_expression(iter, op_prec + 1)?;
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
            "==" | "!=" | "<" | ">" | "<=" | ">=" => 5,
            "&" | "|" | "^" => 6,   // bitwise for spikes
            "<<" | ">>" => 7,
            "+" | "-" => 10,
            "*" | "/" | "%" => 20,
            _ => 0,
        }
    }
}
