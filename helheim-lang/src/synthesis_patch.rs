use crate::ast::{CodeTaal, LiteralValue};
use anyhow::Result;

pub struct GeneralPtxGenerator {
    next_reg: u32,
    next_pred: u32,
    next_label: u32,
    var_map: std::collections::HashMap<String, String>,
    functions_ptx: String,
}

impl GeneralPtxGenerator {
    pub fn new() -> Self {
        Self {
            next_reg: 0,
            next_pred: 0,
            next_label: 0,
            var_map: std::collections::HashMap::new(),
            functions_ptx: String::new(),
        }
    }

    pub fn lower_general(&mut self, code: &CodeTaal) -> Result<String> {
        let mut ptx = String::new();
        ptx.push_str(".version 7.0\n");
        ptx.push_str(".target sm_80\n");
        ptx.push_str(".address_size 64\n\n");

        self.extract_functions(code)?;
        ptx.push_str(&self.functions_ptx);

        ptx.push_str("extern \"C\" .entry main() {\n");
        ptx.push_str("    .reg .f64 %f<1024>;\n");
        ptx.push_str("    .reg .pred %p<1024>;\n");

        self.emit_statement(&mut ptx, code, 1)?;

        ptx.push_str("    ret;\n");
        ptx.push_str("}\n");

        Ok(ptx)
    }

    fn extract_functions(&mut self, node: &CodeTaal) -> Result<()> {
        match node {
            CodeTaal::Block { statements } => {
                for s in statements {
                    self.extract_functions(s)?;
                }
            }
            CodeTaal::FunctionDef { name, params, body } => {
                // Save scope
                let old_map = self.var_map.clone();
                let old_reg = self.next_reg;
                let old_pred = self.next_pred;
                
                self.next_reg = 0;
                self.next_pred = 0;
                self.var_map.clear();

                let mut params_str = Vec::new();
                for (i, p) in params.iter().enumerate() {
                    let p_reg = format!("%f{}", self.next_reg);
                    self.next_reg += 1;
                    self.var_map.insert(p.clone(), p_reg.clone());
                    params_str.push(format!(".reg .f64 {}", p_reg));
                }

                let mut func_ptx = String::new();
                func_ptx.push_str(&format!(".func (.reg .f64 %ret) {} ({}) {{\n", name, params_str.join(", ")));
                func_ptx.push_str("    .reg .f64 %f<1024>;\n");
                func_ptx.push_str("    .reg .pred %p<1024>;\n");

                self.emit_statement(&mut func_ptx, body, 1)?;

                func_ptx.push_str("    mov.f64 %ret, 0f0000000000000000;\n"); // Fallback return
                func_ptx.push_str("    ret;\n}\n\n");
                
                self.functions_ptx.push_str(&func_ptx);

                // Restore scope
                self.var_map = old_map;
                self.next_reg = old_reg;
                self.next_pred = old_pred;
            }
            _ => {}
        }
        Ok(())
    }

    fn emit_statement(&mut self, out: &mut String, stmt: &CodeTaal, indent: usize) -> Result<()> {
        let pad = "    ".repeat(indent);

        match stmt {
            CodeTaal::Block { statements } => {
                for s in statements {
                    self.emit_statement(out, s, indent)?;
                }
            }
            CodeTaal::VarDef { name, value } => {
                let val_reg = self.translate_expression(out, value, indent)?;
                let dst_reg = self.alloc_temp_reg();
                out.push_str(&format!("{}mov.f64 {}, {};\n", pad, dst_reg, val_reg));
                self.var_map.insert(name.clone(), dst_reg);
            }
            CodeTaal::Op { .. } | CodeTaal::FunctionCall { .. } => {
                let _ = self.translate_expression(out, stmt, indent)?;
            }
            CodeTaal::If { condition, then, else_block } => {
                let cond_reg = self.translate_expression(out, condition, indent)?;
                let p = self.alloc_pred_reg();
                
                // Compare with 0.0 (false)
                out.push_str(&format!("{}setp.ne.f64 {}, {}, 0f0000000000000000;\n", pad, p, cond_reg));
                
                let then_label = self.new_label("then");
                let else_label = self.new_label("else");
                let end_label = self.new_label("endif");

                out.push_str(&format!("{}@{} bra {};\n", pad, p, then_label));
                out.push_str(&format!("{}bra {};\n", pad, else_label));

                out.push_str(&format!("{}:\n", then_label));
                self.emit_statement(out, then, indent + 1)?;
                out.push_str(&format!("{}bra {};\n", pad, end_label));

                out.push_str(&format!("{}:\n", else_label));
                if let Some(eb) = else_block {
                    self.emit_statement(out, eb, indent + 1)?;
                }
                out.push_str(&format!("{}:\n", end_label));
            }
            CodeTaal::Loop { condition, body } => {
                let loop_start = self.new_label("loop");
                let loop_end = self.new_label("loop_end");

                out.push_str(&format!("{}:\n", loop_start));
                let cond_reg = self.translate_expression(out, condition, indent)?;
                let p = self.alloc_pred_reg();
                
                out.push_str(&format!("{}setp.eq.f64 {}, {}, 0f0000000000000000;\n", pad, p, cond_reg));
                out.push_str(&format!("{}@{} bra {};\n", pad, p, loop_end));

                self.emit_statement(out, body, indent + 1)?;
                out.push_str(&format!("{}bra {};\n", pad, loop_start));
                out.push_str(&format!("{}:\n", loop_end));
            }
            CodeTaal::Return { value } => {
                if let Some(v) = value {
                    let reg = self.translate_expression(out, v, indent)?;
                    out.push_str(&format!("{}mov.f64 %ret, {};\n", pad, reg));
                }
                out.push_str(&format!("{}ret;\n", pad));
            }
            CodeTaal::FunctionDef { .. } => {
                // Handled in extract_functions
            }
            _ => {
                out.push_str(&format!("{}// Unhandled statement\n", pad));
            }
        }
        Ok(())
    }

    fn translate_expression(&mut self, out: &mut String, expr: &CodeTaal, indent: usize) -> Result<String> {
        let pad = "    ".repeat(indent);
        match expr {
            CodeTaal::Op { left, op, right } => {
                // Logical AND / OR with short-circuiting could be complex, but let's do standard strict evaluation for now
                let l = self.translate_expression(out, left, indent)?;
                let r = self.translate_expression(out, right, indent)?;
                let dst = self.alloc_temp_reg();

                let ptx_op = match op.as_str() {
                    "+" => "add.f64",
                    "-" => "sub.f64",
                    "*" => "mul.f64",
                    "/" => "div.f64",
                    "==" => "setp.eq.f64",
                    "!=" => "setp.ne.f64",
                    ">" => "setp.gt.f64",
                    "<" => "setp.lt.f64",
                    ">=" => "setp.ge.f64",
                    "<=" => "setp.le.f64",
                    "&&" => "and.pred",
                    "||" => "or.pred",
                    _ => "add.f64",
                };

                if op.as_str() == "&&" || op.as_str() == "||" {
                    let p1 = self.alloc_pred_reg();
                    let p2 = self.alloc_pred_reg();
                    let p3 = self.alloc_pred_reg();
                    out.push_str(&format!("{}setp.ne.f64 {}, {}, 0f0000000000000000;\n", pad, p1, l));
                    out.push_str(&format!("{}setp.ne.f64 {}, {}, 0f0000000000000000;\n", pad, p2, r));
                    out.push_str(&format!("{}ref_op {}, {}, {};\n".replace("ref_op", ptx_op), pad, p3, p1, p2));
                    out.push_str(&format!("{}selp.f64 {}, 0f3ff0000000000000, 0f0000000000000000, {};\n", pad, dst, p3));
                } else if ptx_op.starts_with("setp.") {
                    let p = self.alloc_pred_reg();
                    out.push_str(&format!("{}{} {}, {}, {};\n", pad, ptx_op, p, l, r));
                    out.push_str(&format!("{}selp.f64 {}, 0f3ff0000000000000, 0f0000000000000000, {};\n", pad, dst, p));
                } else {
                    out.push_str(&format!("{}{} {}, {}, {};\n", pad, ptx_op, dst, l, r));
                }
                Ok(dst)
            }
            CodeTaal::Literal(LiteralValue::Int(i)) => {
                let dst = self.alloc_temp_reg();
                let bits = (*i as f64).to_bits();
                out.push_str(&format!("{}mov.f64 {}, 0f{:016x};\n", pad, dst, bits));
                Ok(dst)
            }
            CodeTaal::Literal(LiteralValue::Float(f)) => {
                let dst = self.alloc_temp_reg();
                out.push_str(&format!("{}mov.f64 {}, 0f{:016x};\n", pad, dst, f.to_bits()));
                Ok(dst)
            }
            CodeTaal::Literal(LiteralValue::Bool(b)) => {
                let dst = self.alloc_temp_reg();
                let val = if *b { 1.0f64 } else { 0.0f64 };
                out.push_str(&format!("{}mov.f64 {}, 0f{:016x};\n", pad, dst, val.to_bits()));
                Ok(dst)
            }
            CodeTaal::VarGet { name } => {
                if let Some(reg) = self.var_map.get(name) {
                    Ok(reg.clone())
                } else {
                    let dst = self.alloc_temp_reg();
                    out.push_str(&format!("{}mov.f64 {}, 0f0000000000000000;\n", pad, dst));
                    Ok(dst)
                }
            }
            CodeTaal::FunctionCall { name, args } => {
                let mut arg_regs = Vec::new();
                for arg in args {
                    arg_regs.push(self.translate_expression(out, arg, indent)?);
                }
                let dst = self.alloc_temp_reg();
                let args_str = arg_regs.join(", ");
                out.push_str(&format!("{}call ({}), {}, ({});\n", pad, dst, name, args_str));
                Ok(dst)
            }
            _ => {
                let dst = self.alloc_temp_reg();
                out.push_str(&format!("{}mov.f64 {}, 0f0000000000000000;\n", pad, dst));
                Ok(dst)
            }
        }
    }

    fn alloc_temp_reg(&mut self) -> String {
        let reg = format!("%f{}", self.next_reg);
        self.next_reg += 1;
        reg
    }

    fn alloc_pred_reg(&mut self) -> String {
        let reg = format!("%p{}", self.next_pred);
        self.next_pred += 1;
        reg
    }

    fn new_label(&mut self, prefix: &str) -> String {
        let lbl = format!("{}_{}", prefix, self.next_label);
        self.next_label += 1;
        lbl
    }
}
