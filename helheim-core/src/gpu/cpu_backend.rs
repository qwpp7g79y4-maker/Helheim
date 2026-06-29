use dashmap::DashMap;

use rayon::prelude::*;
use crate::gpu::backend::{GpuBackend, GpuPtr, GpuError, TensorInit, GpuCapabilities, CompiledKernel, RawKernelHandle};
use helheim_lang::ast::{GpuKernelDef, Precision, CodeTaal, LiteralValue};

#[derive(Debug)]
pub struct CpuBackend {
    tensors: DashMap<usize, Vec<f32>>,   // Voor nu f32, later generiek
    next_id: std::sync::atomic::AtomicUsize,
}

impl CpuBackend {
    pub fn new() -> Self {
        Self {
            tensors: DashMap::new(),
            next_id: std::sync::atomic::AtomicUsize::new(1),
        }
    }
}

impl GpuBackend for CpuBackend {
    fn name(&self) -> &'static str { "CPU Backend (Rayon 5950X)" }

    fn is_available(&self) -> bool { true }

    fn capabilities(&self) -> GpuCapabilities {
        GpuCapabilities {
            max_workgroup_size: 32 * 8, // simulatie van "workgroups"
            supports_tensor_cores: false,
            vendor: "AMD Ryzen".to_string(),
            max_shared_memory: 0,
        }
    }

    fn allocate_tensor(&self, shape: &[usize], _precision: Precision, init: TensorInit) 
        -> Result<GpuPtr, GpuError> 
    {
        let size = shape.iter().product::<usize>();
        let mut data = vec![0.0f32; size];

        if let TensorInit::Random = init {
            data.iter_mut().for_each(|x| *x = rand::random());
        }

        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.tensors.insert(id, data);

        GpuPtr::new(id).ok_or(GpuError::Internal("Invalid GpuPtr".into()))
    }

    fn compile(&self, kernel: &GpuKernelDef) -> Result<CompiledKernel, GpuError> {
        Ok(CompiledKernel {
            backend_name: self.name(),
            raw_handle: Some(RawKernelHandle::Cpu(kernel.clone())),
            entry_point: kernel.name.clone(),
        })
    }

    fn launch(&self, kernel: &CompiledKernel, args: &[GpuPtr]) -> Result<(), GpuError> {
        let Some(RawKernelHandle::Cpu(def)) = &kernel.raw_handle else {
            return Err(GpuError::Internal("Wrong kernel type".into()));
        };

        tracing::debug!("[CPU 5950X] Executing kernel '{}' on {} threads", 
                def.name, rayon::current_num_threads());

        // Map parameter names to GpuPtrs
        let mut arg_map = std::collections::HashMap::new();
        for (i, param) in def.params.iter().enumerate() {
            if i < args.len() {
                arg_map.insert(param.name.clone(), args[i]);
            }
        }

        // Recursieve AST executie (CPU Interpreter)
        self.execute_ast(&def.body, &arg_map)?;
        
        Ok(())
    }

    fn synchronize(&self) -> Result<(), GpuError> {
        Ok(())
    }

    fn free_tensor(&self, ptr: GpuPtr) -> Result<(), GpuError> {
        self.tensors.remove(&ptr.get());
        Ok(())
    }

    /// For CPU backend we simply fall back to interpreting the AST (already possible for many nodes).
    fn execute_lowered_block(
        &self,
        _code: &CodeTaal,
        _context: &std::collections::HashMap<String, LiteralValue>,
    ) -> Result<Option<f32>, GpuError> {
        tracing::debug!("[CPU Backend] execute_lowered_block: falling back to CPU AST interpretation (pure compute will run on 5950X threads)");
        Err(GpuError::NotAvailable("CPU Backend requires AST interpretation".into()))
    }
}

impl CpuBackend {
    fn execute_ast(&self, ast: &helheim_lang::ast::CodeTaal, arg_map: &std::collections::HashMap<String, GpuPtr>) -> Result<(), GpuError> {
        match ast {
            helheim_lang::ast::CodeTaal::Block { statements } => {
                for stmt in statements {
                    self.execute_ast(stmt, arg_map)?;
                }
            }
            helheim_lang::ast::CodeTaal::GpuOp(op) => {
                match op {
                    helheim_lang::ast::GpuOperation::MatrixMultiplyAccumulate { a, b, c, m, n, k, .. } => {
                        let ptr_a = arg_map.get(a).ok_or_else(|| GpuError::Internal(format!("Arg {} not found", a)))?;
                        let ptr_b = arg_map.get(b).ok_or_else(|| GpuError::Internal(format!("Arg {} not found", b)))?;
                        let ptr_c = arg_map.get(c).ok_or_else(|| GpuError::Internal(format!("Arg {} not found", c)))?;
                        
                        let m = *m as usize;
                        let n = *n as usize;
                        let k = *k as usize;

                        // Clone A and B to avoid DashMap deadlocks when updating C
                        let tensor_a = {
                            let a_ref = self.tensors.get(&ptr_a.get()).ok_or_else(|| GpuError::Internal(format!("Tensor niet gevonden: {:?}", ptr_a.get())))?;
                            a_ref.clone()
                        };
                        let tensor_b = {
                            let b_ref = self.tensors.get(&ptr_b.get()).ok_or_else(|| GpuError::Internal(format!("Tensor niet gevonden: {:?}", ptr_b.get())))?;
                            b_ref.clone()
                        };

                        // Verwijder tijdelijk C uit de map om veilig parallel te muteren
                        let (_, mut tensor_c) = self.tensors.remove(&ptr_c.get()).ok_or_else(|| GpuError::Internal("Tensor C not found".into()))?;

                        tracing::debug!("[CPU 5950X] Computing MatMul ({}x{}x{}) op Rayon...", m, n, k);
                        
                        // Pure brute-force Rayon par_iter_mut over de rijen van C
                        tensor_c.par_chunks_mut(n).enumerate().for_each(|(i, row_c)| {
                            for j in 0..n {
                                let mut sum = 0.0;
                                for p in 0..k {
                                    sum += tensor_a[i * k + p] * tensor_b[p * n + j];
                                }
                                row_c[j] += sum;
                            }
                        });

                        // Plaats C weer terug
                        self.tensors.insert(ptr_c.get(), tensor_c);
                    }
                    helheim_lang::ast::GpuOperation::SubgroupSync => {
                        // Nop op CPU, Rayon threads syncen na par_iter impliciet
                    }
                    _ => {
                        tracing::debug!("[CPU 5950X] Waarschuwing: Onbekende GpuOperation overgeslagen.");
                    }
                }
            }
            _ => {
                tracing::debug!("[CPU 5950X] Waarschuwing: Negeer niet-GpuOp blok.");
            }
        }
        Ok(())
    }
}
