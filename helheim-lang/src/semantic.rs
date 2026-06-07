use crate::ast::CodeTaal;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TypeInfo {
    Int,
    Float,
    String,
    Bool,
    Tensor,
    List, // for 1D/2D spike matrices/tensors
    Void,
    Unknown,
}

impl fmt::Display for TypeInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeInfo::Int => write!(f, "Getal (Int)"),
            TypeInfo::Float => write!(f, "Vloei (Float)"),
            TypeInfo::String => write!(f, "Tekst (String)"),
            TypeInfo::Bool => write!(f, "Waarheid (Bool)"),
            TypeInfo::Tensor => write!(f, "Tensor"),
            TypeInfo::List => write!(f, "Lijst (spike matrix/tensor)"),
            TypeInfo::Void => write!(f, "Niets (Void)"),
            TypeInfo::Unknown => write!(f, "Onbekend"),
        }
    }
}

#[derive(Debug)]
pub enum SemanticError {
    UndefinedVariable(String),
    VariableAlreadyDefined(String),
    TypeMismatch { expected: String, found: String },
    UnsupportedOperation { op: String, ty: String },
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SemanticError::UndefinedVariable(name) => {
                write!(f, "Semantische Fout: Variabele '{}' is niet gedefinieerd in deze scope.", name)
            }
            SemanticError::VariableAlreadyDefined(name) => {
                write!(f, "Semantische Fout: Variabele '{}' is al gedefinieerd in deze scope.", name)
            }
            SemanticError::TypeMismatch { expected, found } => {
                write!(f, "Type Fout: Verwachtte '{}', maar kreeg '{}'.", expected, found)
            }
            SemanticError::UnsupportedOperation { op, ty } => {
                write!(f, "Type Fout: Operatie '{}' wordt niet ondersteund voor type '{}'.", op, ty)
            }
        }
    }
}

impl std::error::Error for SemanticError {}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub ty: TypeInfo,
}

pub struct SymbolTable {
    scopes: Vec<HashMap<String, SymbolInfo>>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()], // Global scope
        }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define(&mut self, name: &str, ty: TypeInfo) -> Result<(), SemanticError> {
        let current_scope = self.scopes.last_mut().unwrap();
        current_scope.insert(name.to_string(), SymbolInfo { name: name.to_string(), ty });
        Ok(())
    }

    pub fn resolve(&self, name: &str) -> Option<&SymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }
}

pub struct SemanticAnalyzer {
    symbols: SymbolTable,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            symbols: SymbolTable::new(),
        }
    }

    pub fn analyze(ast: &mut Vec<CodeTaal>) -> Result<(), SemanticError> {
        let mut analyzer = Self::new();
        for node in ast {
            analyzer.visit(node)?;
        }
        Ok(())
    }

    fn visit(&mut self, node: &CodeTaal) -> Result<TypeInfo, SemanticError> {
        match node {
            CodeTaal::Literal(val) => {
                match val {
                    crate::ast::LiteralValue::Int(_) => Ok(TypeInfo::Int),
                    crate::ast::LiteralValue::Float(_) => Ok(TypeInfo::Float),
                    crate::ast::LiteralValue::String(_) => Ok(TypeInfo::String),
                    crate::ast::LiteralValue::Bool(_) => Ok(TypeInfo::Bool),
                    crate::ast::LiteralValue::List(_) => Ok(TypeInfo::List), // 2D spike tensors supported as lists
                }
            }
            CodeTaal::VarDef { name, value } => {
                let ty = self.visit(value)?;
                self.symbols.define(name, ty.clone())?;
                Ok(TypeInfo::Void)
            }
            CodeTaal::VarGet { name } => {
                if let Some(info) = self.symbols.resolve(name) {
                    Ok(info.ty.clone())
                } else {
                    Err(SemanticError::UndefinedVariable(name.to_string()))
                }
            }
            CodeTaal::Block { statements } => {
                self.symbols.enter_scope();
                let mut last_type = TypeInfo::Void;
                for stmt in statements {
                    last_type = self.visit(stmt)?;
                }
                self.symbols.exit_scope();
                Ok(last_type)
            }
            CodeTaal::If { condition, then, else_block } => {
                let cond_ty = self.visit(condition)?;
                if cond_ty != TypeInfo::Bool && cond_ty != TypeInfo::Unknown {
                    return Err(SemanticError::TypeMismatch { expected: "Waarheid (Bool)".to_string(), found: cond_ty.to_string() });
                }
                
                self.symbols.enter_scope();
                let ret_ty = self.visit(then)?;
                self.symbols.exit_scope();

                if let Some(e) = else_block {
                    self.symbols.enter_scope();
                    let _else_ty = self.visit(e)?;
                    self.symbols.exit_scope();
                    // Optional: check if ret_ty == else_ty
                }
                Ok(ret_ty)
            }
            CodeTaal::Loop { condition, body } => {
                let cond_ty = self.visit(condition)?;
                if cond_ty != TypeInfo::Bool && cond_ty != TypeInfo::Unknown {
                    return Err(SemanticError::TypeMismatch { expected: "Waarheid (Bool)".to_string(), found: cond_ty.to_string() });
                }
                
                self.symbols.enter_scope();
                self.visit(body)?;
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::ForEach { iterator, iterable, body } => {
                let _iter_ty = self.visit(iterable)?;
                
                self.symbols.enter_scope();
                // We assume iterable is a collection, iterator gets Unknown or inferred type
                self.symbols.define(iterator, TypeInfo::Unknown)?;
                self.visit(body)?;
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::FunctionDef { name, params, body } => {
                self.symbols.define(name, TypeInfo::Unknown)?;
                
                self.symbols.enter_scope();
                for param in params {
                    self.symbols.define(param, TypeInfo::Unknown)?;
                }
                self.visit(body)?;
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::FunctionCall { name: _, args } => {
                for arg in args {
                    self.visit(arg)?;
                }
                Ok(TypeInfo::Unknown)
            }
            CodeTaal::Op { left, op, right } => {
                let l_ty = self.visit(left)?;
                let r_ty = self.visit(right)?;
                
                // Simplified type checking
                if l_ty == TypeInfo::Unknown || r_ty == TypeInfo::Unknown {
                    return Ok(TypeInfo::Unknown);
                }

                match op.as_str() {
                    "+" | "-" | "*" | "/" | "%" => {
                        if l_ty == TypeInfo::Int && r_ty == TypeInfo::Int {
                            Ok(TypeInfo::Int)
                        } else if (l_ty == TypeInfo::Float || l_ty == TypeInfo::Int) && (r_ty == TypeInfo::Float || r_ty == TypeInfo::Int) {
                            Ok(TypeInfo::Float)
                        } else if l_ty == TypeInfo::String && op == "+" {
                            Ok(TypeInfo::String)
                        } else if l_ty == TypeInfo::Tensor && r_ty == TypeInfo::Tensor {
                            Ok(TypeInfo::Tensor) // Tensor math
                        } else {
                            Err(SemanticError::UnsupportedOperation { op: op.clone(), ty: format!("{} en {}", l_ty, r_ty) })
                        }
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        Ok(TypeInfo::Bool)
                    }
                    _ => Ok(TypeInfo::Unknown)
                }
            }
            CodeTaal::Return { value } => {
                if let Some(v) = value {
                    self.visit(v)
                } else {
                    Ok(TypeInfo::Void)
                }
            }
            CodeTaal::ArrayPush { array_name, value: _ } => {
                if self.symbols.resolve(array_name).is_none() {
                    return Err(SemanticError::UndefinedVariable(array_name.to_string()));
                }
                Ok(TypeInfo::Void)
            }
            CodeTaal::ArrayRemove { array_name, index: _ } => {
                if self.symbols.resolve(array_name).is_none() {
                    return Err(SemanticError::UndefinedVariable(array_name.to_string()));
                }
                Ok(TypeInfo::Void)
            }
            CodeTaal::Concurrent { statements } => {
                self.symbols.enter_scope();
                for stmt in statements {
                    self.visit(stmt)?;
                }
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::Daemon { body } => {
                self.symbols.enter_scope();
                self.visit(body)?;
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::TryCatch { try_block, catch_block, error_var } => {
                self.symbols.enter_scope();
                self.visit(try_block)?;
                self.symbols.exit_scope();

                self.symbols.enter_scope();
                if let Some(e) = error_var {
                    self.symbols.define(e, TypeInfo::String)?; // Errors usually strings
                }
                self.visit(catch_block)?;
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::GpuKernel(kernel) => {
                self.symbols.define(&kernel.name, TypeInfo::Unknown)?;
                self.symbols.enter_scope();
                for param in &kernel.params {
                    let p_ty = match &param.ty {
                        crate::ast::GpuType::Tensor(_) => TypeInfo::Tensor,
                        crate::ast::GpuType::Scalar(_) => TypeInfo::Float,
                        _ => TypeInfo::Unknown,
                    };
                    self.symbols.define(&param.name, p_ty)?;
                }
                self.visit(&kernel.body)?;
                self.symbols.exit_scope();
                Ok(TypeInfo::Void)
            }
            CodeTaal::GpuOp(op) => {
                match op {
                    crate::ast::GpuOperation::SubgroupAdd { value } => { self.visit(value)?; }
                    crate::ast::GpuOperation::SubgroupShuffle { value, lane } => {
                        self.visit(value)?;
                        self.visit(lane)?;
                    }
                    crate::ast::GpuOperation::SharedLoad { name, indices } => {
                        if self.symbols.resolve(name).is_none() {
                            return Err(SemanticError::UndefinedVariable(name.to_string()));
                        }
                        for idx in indices {
                            self.visit(idx)?;
                        }
                    }
                    crate::ast::GpuOperation::SharedStore { name, indices, value } => {
                        if self.symbols.resolve(name).is_none() {
                            return Err(SemanticError::UndefinedVariable(name.to_string()));
                        }
                        for idx in indices {
                            self.visit(idx)?;
                        }
                        self.visit(value)?;
                    }
                    crate::ast::GpuOperation::MatrixMultiplyAccumulate { a, b, c, .. } => {
                        let a_sym = self.symbols.resolve(a).cloned();
                        let b_sym = self.symbols.resolve(b).cloned();
                        let c_sym = self.symbols.resolve(c).cloned();
                        
                        if let Some(info) = a_sym { if info.ty != TypeInfo::Tensor && info.ty != TypeInfo::Unknown { return Err(SemanticError::TypeMismatch { expected: "Tensor".to_string(), found: info.ty.to_string() }); } } else { return Err(SemanticError::UndefinedVariable(a.to_string())); }
                        if let Some(info) = b_sym { if info.ty != TypeInfo::Tensor && info.ty != TypeInfo::Unknown { return Err(SemanticError::TypeMismatch { expected: "Tensor".to_string(), found: info.ty.to_string() }); } } else { return Err(SemanticError::UndefinedVariable(b.to_string())); }
                        if let Some(info) = c_sym { if info.ty != TypeInfo::Tensor && info.ty != TypeInfo::Unknown { return Err(SemanticError::TypeMismatch { expected: "Tensor".to_string(), found: info.ty.to_string() }); } } else { return Err(SemanticError::UndefinedVariable(c.to_string())); }
                    }
                    _ => {}
                }
                Ok(TypeInfo::Void)
            }
            CodeTaal::HttpOp { url, .. } => {
                // Haal/fetch: altijd String resultaat (response body)
                let _ = self.visit(url)?;
                Ok(TypeInfo::String)
            }
            CodeTaal::FileOp { action, path, content } => {
                let _ = self.visit(path)?;
                if let Some(c) = content {
                    let _ = self.visit(c)?;
                }
                match action.as_str() {
                    "read" => Ok(TypeInfo::String),
                    "write" => Ok(TypeInfo::Void),
                    _ => Ok(TypeInfo::Unknown),
                }
            }
            CodeTaal::Print { .. } => {
                Ok(TypeInfo::Void)
            }
            CodeTaal::Send { .. } | CodeTaal::SysOp { .. } | CodeTaal::Encrypt { .. } | CodeTaal::Gebruik { .. } | CodeTaal::RuneOp { .. } => {
                Ok(TypeInfo::Void)
            }
            _ => {
                Ok(TypeInfo::Unknown)
            }
        }
    }
}
