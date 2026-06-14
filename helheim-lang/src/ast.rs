//! Helheim Abstract Syntax Tree (AST)
//! This module contains the core language representation (CodeTaal).

use serde::{Deserialize, Serialize};

/// Strongly typed literal values.
/// This replaces the old `Literal(String)` which forced guessing at runtime/PTX time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    List(Vec<LiteralValue>),
}

impl std::fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralValue::Int(i) => write!(f, "{}", i),
            LiteralValue::Float(fl) => {
                if fl.fract() == 0.0 {
                    write!(f, "{}.0", fl)
                } else {
                    write!(f, "{}", fl)
                }
            },
            LiteralValue::String(s) => write!(f, "{}", s),
            LiteralValue::Bool(b) => write!(f, "{}", if *b { "waar" } else { "onwaar" }),
            LiteralValue::List(items) => {
                let s = items.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(", ");
                write!(f, "[{}]", s)
            }
        }
    }
}

/// The core AST node for Helheim.
/// This represents both statements and expressions in a unified way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CodeTaal {
    // --- LANGUAGE CORE ---
    Gebruik { path: String },
    VarDef { name: String, value: Box<CodeTaal> },
    VarGet { name: String },

    ArrayPush { array_name: String, value: String },
    ArrayRemove { array_name: String, index: String },

    Block { statements: Vec<CodeTaal> },
    Concurrent { statements: Vec<CodeTaal> },
    Daemon { body: Box<CodeTaal> },
    HelBlock { raw_code: String },

    Loop {
        condition: Box<CodeTaal>,
        body: Box<CodeTaal>,
    },
    ForEach {
        iterator: String,
        iterable: Box<CodeTaal>,
        body: Box<CodeTaal>,
    },
    If {
        condition: Box<CodeTaal>,
        then: Box<CodeTaal>,
        else_block: Option<Box<CodeTaal>>,
    },

    FunctionDef {
        name: String,
        params: Vec<String>,
        body: Box<CodeTaal>,
    },
    /// Improved: arguments are now proper AST nodes instead of raw strings.
    FunctionCall {
        name: String,
        args: Vec<CodeTaal>,
    },
    /// Improved: return value is now an optional expression instead of String.
    Return {
        value: Option<Box<CodeTaal>>,
    },

    TryCatch {
        try_block: Box<CodeTaal>,
        catch_block: Box<CodeTaal>,
        error_var: Option<String>,
    },

    ModelDef {
        name: String,
        fields: Vec<String>,
    },
    ModelInit {
        model_name: String,
        args: Vec<String>,
    },

    Throw { message: String },

    Op {
        left: Box<CodeTaal>,
        op: String,
        right: Box<CodeTaal>,
    },

    /// Strongly typed literal (major improvement over Literal(String))
    Literal(LiteralValue),

    ListLiteral { items: Vec<LiteralValue> },
    MatrixLiteral { rows: Vec<Vec<LiteralValue>> },

    // --- GPU SECTION ---
    GpuKernel(GpuKernelDef),
    GpuOp(GpuOperation),

    // --- HOST OPERATIONS (keep for now) ---
    MatMul { m: usize, n: usize, k: usize },
    TensorAdd { m: usize, n: usize },
    TensorRelu { m: usize, n: usize },
    VectorAdd { len: usize },
    Chaos { intensity: u8, duration_ms: u64 },
    Send { target: String, payload: String },
    Encrypt { algo: String, data: String },
    FileOp {
        action: String,
        path: Box<CodeTaal>,
        content: Option<Box<CodeTaal>>,
    },
    SysOp { command: String },
    HttpOp { method: String, url: Box<CodeTaal> },
    RuneOp { command: String },
    Print { message: String },
}

// --- GPU Related Types (temporarily kept here) ---

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuKernelDef {
    pub name: String,
    pub attributes: Vec<KernelAttribute>,
    pub params: Vec<GpuParam>,
    pub body: Box<CodeTaal>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KernelAttribute {
    WorkgroupSize(u32),
    SubgroupSize(u32),
    UseTensorCores(bool),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuParam {
    pub name: String,
    pub ty: GpuType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuType {
    Tensor(Precision),
    Shared(Precision, Vec<usize>),
    Pointer(Box<GpuType>),
    Scalar(Precision),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Precision {
    F16,
    BF16,
    F32,
    I8,
    I32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GpuOperation {
    SubgroupSync,
    SubgroupAdd { value: Box<CodeTaal> },
    SubgroupShuffle { value: Box<CodeTaal>, lane: Box<CodeTaal> },
    SharedLoad { name: String, indices: Vec<Box<CodeTaal>> },
    SharedStore { name: String, indices: Vec<Box<CodeTaal>>, value: Box<CodeTaal> },
    MatrixMultiplyAccumulate {
        a: String,
        b: String,
        c: String,
        m: usize,
        n: usize,
        k: usize,
        precision: Precision,
    },
}