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
    /// Raw bytes for network primitives and binary data. C-style.
    Bytes(Vec<u8>),
    /// Raw foreign pointer for unsafe memory access (Zero-cost FFI).
    Pointer(u64),
    /// Void / Empty value
    Void,
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
            LiteralValue::Bytes(b) => {
                // Rock-solid display: prefer printable as b"..." else hex
                if let Ok(s) = std::str::from_utf8(b) {
                    if s.chars().all(|c| c.is_ascii_graphic() || c == ' ' || c == '\r' || c == '\n') {
                        return write!(f, "b\"{}\"", s);
                    }
                }
                let hex: Vec<String> = b.iter().map(|byte| format!("{:02x}", byte)).collect();
                write!(f, "b[{}]", hex.join(" "))
            }
            LiteralValue::Pointer(addr) => write!(f, "ptr(0x{:x})", addr),
            LiteralValue::Void => write!(f, "niets"),
        }
    }
}

/// The core AST node for Helheim.
/// This represents both statements and expressions in a unified way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CodeTaal {
    // --- LANGUAGE CORE ---
    Gebruik { path: String, module_naam: Option<String> },
    Module { name: String, body: Vec<CodeTaal> },
    VarDef { name: String, value: Box<CodeTaal> },
    VarGet { name: String },
    /// Injected by parser to track source locations for error reporting
    LocationMarker { line: usize, col: usize },

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
        is_pub: bool,
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
    // Legacy string-based (to be phased out)
    TcpOp { action: String, host: Box<CodeTaal>, data: Option<Box<CodeTaal>> },

    // === NEW PRIMITIVES-FIRST TCP NODES (C-style, raw bytes, handles) ===
    /// tcp_luister "0.0.0.0:port"  → listener handle
    TcpListen { addr: Box<CodeTaal> },
    /// tcp_accepteer listener_handle → new connected stream handle
    TcpAccept { listener: Box<CodeTaal> },
    /// tcp_verbind "host:port" → stream handle
    TcpConnect { addr: Box<CodeTaal> },
    /// tcp_stuur socket_handle, data   (data: Bytes | String | List)
    TcpSend { socket: Box<CodeTaal>, data: Box<CodeTaal> },
    /// tcp_ontvang socket_handle [max_bytes]
    TcpReceive { socket: Box<CodeTaal>, max_bytes: Option<Box<CodeTaal>> },
    /// tcp_sluit socket_handle
    TcpClose { socket: Box<CodeTaal> },

    RuneOp { command: String },
    Print { message: String },

    // === ACTOR / MESSAGE-PASSING PRIMITIVES (Vraag 2) ===
    /// spawn [name] { body }
    /// Creates a new isolated actor ("Ziel") with its own execution context.
    Spawn {
        name: Option<String>,
        body: Box<CodeTaal>,
    },

    // === INLINE PTX / ASM (Vraag 1) ===
    /// ptx { ... } or asm { ... } as first-class lowering.
    /// target: "ptx" | "x86" | etc.
    /// code: raw assembly/PTX source.
    /// inputs: bindings from Helheim vars/expressions to asm params.
    /// outputs: list of output param names that will be written back.
    InlineAssembly {
        target: String,
        code: String,
        inputs: Vec<(String, Box<CodeTaal>)>,
        outputs: Vec<String>,
        clobbers: Vec<String>,
        fallback: Option<Box<CodeTaal>>,
    },
    /// stuur_bericht <target> <message>
    /// Sends a message (HelValue) to an actor by ID, name, or remote reference.
    SendMessage {
        target: Box<CodeTaal>,
        message: Box<CodeTaal>,
    },
    /// ontvang <var> [timeout] { body }
    /// Receives a message into <var> and executes body. Blocks the actor until message or timeout.
    Receive {
        var: String,
        timeout: Option<Box<CodeTaal>>,
        body: Box<CodeTaal>,
    },

    // === EFFECTS + LINEAR CAPABILITIES + CONTINUATIONS (Vraag 6) ===
    /// Effect declaration (bijv. "effect Tcp { bind, send, recv }")
    EffectDef {
        name: String,
        operations: Vec<String>,
    },
    /// Perform an effect operation
    Perform {
        effect: String,           // "Tcp"
        operation: String,        // "send"
        args: Vec<CodeTaal>,
    },
    /// Handle effects (delimited continuation)
    Handle {
        effect: String,
        handlers: Vec<(String, Box<CodeTaal>)>,  // operation -> handler body
        body: Box<CodeTaal>,
    },
    /// Resume a continuation (binnen een handler)
    Resume {
        continuation: Box<CodeTaal>,   // de continuation waarde
        value: Box<CodeTaal>,
    },
    /// Linear resource / capability type (voor het type-systeem)
    LinearResource {
        kind: String,   // "TcpStream", "GpuBuffer", "ActorHandle", etc.
        id: Box<CodeTaal>,
    },
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