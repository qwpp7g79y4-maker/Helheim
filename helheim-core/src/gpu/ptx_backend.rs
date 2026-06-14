use std::sync::{Arc, atomic::{AtomicUsize, Ordering}, Mutex};
use dashmap::DashMap;
use cudarc::driver::{CudaContext, CudaSlice, LaunchConfig, PushKernelArg};
use crate::gpu::backend::{
    GpuBackend, GpuPtr, GpuError, TensorInit, GpuCapabilities, CompiledKernel
};
use helheim_lang::ast::{GpuKernelDef, Precision, CodeTaal};

pub struct PtxBackend {
    context: Arc<CudaContext>,
    tensors: DashMap<usize, CudaSlice<f32>>,    // Lock-free concurrent map
    next_id: AtomicUsize,
    // Cache PTX source and the actual loaded CUDA module for real launches.
    kernel_cache: DashMap<String, String>, 
    modules: DashMap<String, std::sync::Arc<cudarc::driver::CudaModule>>,

    // VRAM Memory Pool / Ringbuffer for lowered block result buffers (and future input buffers).
    // This replaces per-launch alloc_zeros::<f32>(1) to eliminate VRAM allocation overhead
    // during high-frequency evaluation loops.
    result_pool: Mutex<Vec<CudaSlice<f32>>>,
    result_pool_index: AtomicUsize,
    result_pool_size: usize,
}

impl PtxBackend {
    pub fn new() -> anyhow::Result<Self> {
        let context = CudaContext::new(0)?;
        let stream = context.default_stream();

        // Pre-allocate a ringbuffer of result buffers for lowered PTX launches.
        // Each slot is 1024 f32 to support 2D matrix output and bit-packed results via multiple PTX stores.
        let result_pool_size = 512;
        let mut pool_vec = Vec::with_capacity(result_pool_size);
        for _ in 0..result_pool_size {
            let buf = stream.alloc_zeros::<f32>(1024)
                .map_err(|e| anyhow::anyhow!("Failed to pre-alloc result pool buffer: {}", e))?;
            pool_vec.push(buf);
        }

        Ok(Self {
            context,
            tensors: DashMap::new(),
            next_id: AtomicUsize::new(1),
            kernel_cache: DashMap::new(),
            modules: DashMap::new(),
            result_pool: Mutex::new(pool_vec),
            result_pool_index: AtomicUsize::new(0),
            result_pool_size,
        })
    }
}

impl GpuBackend for PtxBackend {
    fn name(&self) -> &'static str { "PTX Backend" }

    fn is_available(&self) -> bool { true }

    fn capabilities(&self) -> GpuCapabilities {
        GpuCapabilities {
            max_workgroup_size: 1024,
            supports_tensor_cores: true,
            vendor: "NVIDIA".to_string(),
            max_shared_memory: 49152, // 48KB typisch
        }
    }

    fn allocate_tensor(
        &self,
        shape: &[usize],
        _precision: Precision,        // TODO: later generiek maken
        init: TensorInit,
    ) -> Result<GpuPtr, GpuError> {
        let size = shape.iter().product::<usize>();
        let stream = self.context.default_stream();
        
        let mut slice = stream.alloc_zeros::<f32>(size)
            .map_err(|e: cudarc::driver::DriverError| GpuError::AllocationFailed(e.to_string()))?;

        if let TensorInit::Random = init {
            let host_data: Vec<f32> = (0..size).map(|_| rand::random()).collect();
            stream.memcpy_htod(&host_data, &mut slice)
                .map_err(|e: cudarc::driver::DriverError| GpuError::AllocationFailed(e.to_string()))?;
        }

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.tensors.insert(id, slice);

        GpuPtr::new(id).ok_or_else(|| GpuError::Internal("Invalid pointer".into()))
    }

    fn compile(&self, kernel: &GpuKernelDef) -> Result<CompiledKernel, GpuError> {
        // Probeer cache eerst
        if self.kernel_cache.contains_key(&kernel.name) {
            return Ok(CompiledKernel {
                backend_name: self.name(),
                entry_point: kernel.name.clone(),
                raw_handle: None,
            });
        }

        // Synthetiseren via de nieuwe AST PTX Visitor
        let ptx_source = helheim_lang::synthesis::KernelSynthesisEngine::synthesize_gpu_kernel(kernel)
            .map_err(|e| GpuError::CompilationFailed(format!("PTX Synthesis Error: {:?}", e)))?;

        println!("[PTX SYNTHESIZER] Generated Raw PTX for '{}':\n{}", kernel.name, ptx_source);

        // Load the raw PTX (assembly) directly. Capture the module for later launch.
        let ptx_opts = cudarc::nvrtc::Ptx::from_src(ptx_source.clone());
        
        let module = self.context.load_module(ptx_opts)
            .map_err(|e: cudarc::driver::DriverError| GpuError::CompilationFailed(e.to_string()))?;

        // Cache both source and the live module (note: load_module returns Arc<Module>)
        self.kernel_cache.insert(kernel.name.clone(), ptx_source);
        self.modules.insert(kernel.name.clone(), module);

        Ok(CompiledKernel {
            backend_name: self.name(),
            entry_point: kernel.name.clone(),
            raw_handle: None,
        })
    }

    fn launch(&self, kernel: &CompiledKernel, args: &[GpuPtr]) -> Result<(), GpuError> {
        // Real launch using stored module (closes the JIT pipeline for GpuKernel cases)
        let module_ref = self.modules.get(&kernel.entry_point)
            .ok_or_else(|| GpuError::ExecutionFailed(format!("No loaded module for entry '{}'. Did you call compile first?", kernel.entry_point)))?;

        let module = module_ref.value().clone(); // Arc<CudaModule>
        let f = module.load_function(&kernel.entry_point)
            .map_err(|e| GpuError::ExecutionFailed(format!("Failed to load function '{}': {}", kernel.entry_point, e)))?;

        let stream = self.context.default_stream();

        // Default launch config for simple / lowered kernels. 
        // For kernels with tensor args we would map GpuPtr -> actual slices and pass them.
        // For now support the no-arg or simple case (many lowered blocks start this way).
        let cfg = LaunchConfig {
            grid_dim: (1, 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        };

        println!("[Helheim PTX] Real launch of '{}' (grid 1x1x1, block 256) with {} tensor args", kernel.entry_point, args.len());

        unsafe {
            let mut builder = stream.launch_builder(&f);
            // TODO: map args to real device pointers when GpuPtrs are passed for kernels that expect them.
            // For basic lowered blocks and current GpuKernel usage in executor (empty args), this suffices.
            builder.launch(cfg)
                .map_err(|e| GpuError::ExecutionFailed(format!("Kernel launch failed: {}", e)))?;
        }

        Ok(())
    }

    fn synchronize(&self) -> Result<(), GpuError> {
        self.context.default_stream().synchronize()
            .map_err(|e: cudarc::driver::DriverError| GpuError::ExecutionFailed(e.to_string()))
    }

    fn free_tensor(&self, ptr: GpuPtr) -> Result<(), GpuError> {
        self.tensors.remove(&ptr.get());
        Ok(())
    }

    /// The key piece for the "Echte Launch van de Lowered Blocks".
    /// Takes any CodeTaal (Block, FunctionDef, etc.), forces it through the PtxGenerator
    /// (which now handles general code via lower_general producing "hel_lowered" entry),
    /// loads the PTX with cudarc, and launches it on the real GPU (RTX 5060 Ti / 3060 etc).
    fn execute_lowered_block(
        &self,
        code: &CodeTaal,
        context: &std::collections::HashMap<String, helheim_lang::ast::LiteralValue>,
    ) -> Result<Option<f32>, GpuError> {
        // 1. Synthesize -> PTX with context binding (external vars become input params)
        let ptx_source = helheim_lang::synthesis::KernelSynthesisEngine::synthesize_lowered_with_context(code.clone(), context)
            .map_err(|e| GpuError::CompilationFailed(format!("Lowered PTX synthesis failed: {}", e)))?;

        println!("[PTX LOWERED LAUNCH] Generated PTX for general block/function with context (first 800 chars):\n{}", &ptx_source[..ptx_source.len().min(800)]);

        // 2. Load as raw PTX assembly (not C++ source)
        let ptx = cudarc::nvrtc::Ptx::from_src(ptx_source);
        let module = self.context.load_module(ptx)
            .map_err(|e| GpuError::CompilationFailed(format!("Failed to load lowered PTX module: {}", e)))?;

        // 3. The general lowering produces the entry point "hel_lowered"
        let f = module.load_function("hel_lowered")
            .map_err(|e| GpuError::ExecutionFailed(format!("hel_lowered entry not found in lowered PTX: {}", e)))?;

        // 4. Fire the kernel with a reasonable config for scalar/logic blocks
        let stream = self.context.default_stream();
        let cfg = LaunchConfig {
            grid_dim: (1, 1, 1),
            block_dim: (256, 1, 1),
            shared_mem_bytes: 0,
        };

        // RESULT PROPAGATION via VRAM Ringbuffer / Pool (no more on-the-fly alloc_zeros).
        // Grab a pre-allocated slot from the ringbuffer. The guard lives until after sync.
        let mut pool_guard = self.result_pool.lock()
            .map_err(|e| GpuError::Internal(format!("Result pool lock poisoned: {}", e)))?;
        let pool_idx = self.result_pool_index.fetch_add(1, Ordering::Relaxed) % self.result_pool_size;
        let out_buf = &mut pool_guard[pool_idx];

        // Prepare input args in the same order as emitted in lower_general_with_context (sorted names)
        let mut input_names: Vec<String> = context.keys().cloned().collect();
        input_names.sort();

        println!("[PTX LOWERED LAUNCH] Launching 'hel_lowered' on real CUDA (grid=1x1x1, block=256x1x1) with {} context inputs ...", input_names.len());

        unsafe {
            let mut builder = stream.launch_builder(&f);
            // 1. result pointer (device buffer)
            builder.arg(&mut *out_buf);

            // 2. context input scalars (all as f32 for context binding v1)
            // Collect first so the values live until launch.
            let input_values: Vec<f32> = input_names
                .iter()
                .map(|name| {
                    context.get(name).map(|val| match val {
                        helheim_lang::ast::LiteralValue::Float(f) => *f as f32,
                        helheim_lang::ast::LiteralValue::Int(i) => *i as f32,
                        _ => 0.0,
                    }).unwrap_or(0.0)
                })
                .collect();

            for v in &input_values {
                builder.arg(v);
            }

            builder.launch(cfg)
                .map_err(|e| GpuError::ExecutionFailed(format!("Lowered block launch failed: {}", e)))?;
        }

        stream.synchronize()
            .map_err(|e| GpuError::ExecutionFailed(format!("Synchronize after lowered launch: {}", e)))?;

        // Fetch the result back to host memory
        let mut host_result = vec![0.0f32; 1];
        stream.memcpy_dtoh(&*out_buf, &mut host_result)
            .map_err(|e| GpuError::ExecutionFailed(format!("Failed to copy result from GPU: {}", e)))?;

        let result_val = host_result[0];

        println!("[PTX LOWERED LAUNCH] ✅ Kernel executed on GPU. Fetched Result: {}", result_val);
        Ok(Some(result_val))
    }
}
