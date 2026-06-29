use std::num::NonZeroUsize;
use thiserror::Error;
use helheim_lang::ast::{CodeTaal, GpuKernelDef, Precision};

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("Backend niet beschikbaar: {0}")]
    NotAvailable(String),
    #[error("Compilatie mislukt: {0}")]
    CompilationFailed(String),
    #[error("Allocatie mislukt: {0}")]
    AllocationFailed(String),
    #[error("Executie mislukt: {0}")]
    ExecutionFailed(String),
    #[error("Interne fout: {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GpuPtr {
    id: NonZeroUsize,
    generation: u32,
}

impl GpuPtr {
    pub fn new(id: usize) -> Option<Self> {
        NonZeroUsize::new(id).map(|id| GpuPtr { id, generation: 0 })
    }

    pub fn get(&self) -> usize {
        self.id.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TensorInit {
    Empty,
    Zeros,
    Random,
}

#[derive(Debug, Clone)]
pub struct GpuCapabilities {
    pub max_workgroup_size: u32,
    pub supports_tensor_cores: bool,
    pub vendor: String,
    pub max_shared_memory: u32,
}

pub trait GpuBackend: Send + Sync {
    fn name(&self) -> &'static str;
    fn is_available(&self) -> bool;
    fn capabilities(&self) -> GpuCapabilities;

    fn allocate_tensor(
        &self,
        shape: &[usize],
        precision: Precision,
        init: TensorInit,
    ) -> Result<GpuPtr, GpuError>;

    fn compile(&self, kernel: &GpuKernelDef) -> Result<CompiledKernel, GpuError>;
    fn launch(&self, kernel: &CompiledKernel, args: &[GpuPtr]) -> Result<(), GpuError>;
    fn synchronize(&self) -> Result<(), GpuError>;
    fn free_tensor(&self, ptr: GpuPtr) -> Result<(), GpuError>;

    /// Launch a general lowered block (from Block/FunctionDef)
    /// that was turned into PTX via PtxGenerator::lower_general (entry "hel_lowered").
    /// This closes the full loop: Script -> AST -> PTX -> CUDA launch.
    fn execute_lowered_block(
        &self,
        _code: &CodeTaal,
        _context: &std::collections::HashMap<String, helheim_lang::ast::LiteralValue>,
    ) -> Result<Option<f32>, GpuError> {
        Err(GpuError::NotAvailable("execute_lowered_block not supported on this backend".to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct CompiledKernel {
    pub backend_name: &'static str,
    pub entry_point: String,
    pub raw_handle: Option<RawKernelHandle>, // We store AST here for CPU, empty for PTX
}

#[derive(Debug, Clone)]
pub enum RawKernelHandle {
    Cpu(GpuKernelDef),
}
