use crate::ast::{CodeTaal, GpuKernelDef, GpuOperation, LiteralValue};
use anyhow::Result;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::sync::Mutex;

lazy_static! {
    /// Kernel Cache: Het geheugen van de Reactor.
    /// Slaat reeds gegenereerde PTX op om herhalende taken (loops) instant te maken.
    static ref KERNEL_CACHE: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
}

pub struct KernelSynthesisEngine;

impl KernelSynthesisEngine {
    /// "Synthesize": Laat het abstracte zaadje groeien tot concrete machine-code (PTX).
    pub fn synthesize(seed: CodeTaal) -> Result<String> {
        let cache_key = match serde_json::to_string(&seed) {
            Ok(k) => k,
            Err(_) => String::new(),
        };

        if !cache_key.is_empty() {
            if let Ok(cache) = KERNEL_CACHE.lock() {
                if let Some(ptx) = cache.get(&cache_key) {
                    tracing::debug!("[SYNTHESIS]: PTX retrieved from Kernel Cache!");
                    return Ok(ptx.clone());
                }
            }
        }

        tracing::debug!("[SYNTHESIS]: Starting synthesis (cache miss)...");
        let ptx = match seed {
            CodeTaal::GpuKernel(ref kernel_def) => Self::synthesize_gpu_kernel(kernel_def)?,
            CodeTaal::MatMul { m, n, k } => Self::generate_matmul_ptx(m, n, k),
            CodeTaal::TensorAdd { m, n } => Self::generate_tensor_add_ptx(m, n),
            CodeTaal::TensorRelu { m, n } => Self::generate_tensor_relu_ptx(m, n),
            CodeTaal::VectorAdd { len } => Self::generate_vector_add_ptx(len),
            CodeTaal::Chaos {
                intensity,
                duration_ms,
            } => Self::generate_chaos_ptx(intensity, duration_ms),
            CodeTaal::Send {
                ref target,
                ref payload,
            } => {
                tracing::debug!("[NETWORK]: Preparing packet for {}: '{}'", target, payload);
                format!(
                    "// HOST_OP: SEND -> {} (Payload size: {})",
                    target,
                    payload.len()
                )
            }
            CodeTaal::HelBlock { ref raw_code } => {
                tracing::debug!(
                    "[SYNTHESIS]: Hel-modus detectie. JIT compilatie van ruwe bare-metal logica."
                );
                // We return the raw code directly. The execution engine will pass it to NVRTC.
                format!("// HEL_BLOCK_START\n{}\n// HEL_BLOCK_END", raw_code)
            }
            CodeTaal::InlineAssembly { ref target, ref code, .. } if target == "ptx" => {
                let mut ptx = String::new();
                ptx.push_str("    // === Inline PTX start ===\n");
                for line in code.lines() {
                    ptx.push_str(&format!("    {}\n", line.trim()));
                }
                ptx.push_str("    // === Inline PTX end ===\n");
                ptx
            }
            CodeTaal::Encrypt { ref algo, data: _ } => {
                tracing::debug!("[SECURITY]: Encrypting data with {}...", algo);
                format!("// HOST_OP: ENCRYPT (algo: {})", algo)
            }
            CodeTaal::FileOp {
                ref action,
                ref path,
                ref content,
            } => match action.as_str() {
                "read" => {
                    format!("// HOST_OP: FS_READ -> {:?}", path)
                }
                "write" => {
                    if let Some(_c) = content {
                        format!("// HOST_OP: FS_WRITE -> {:?}", path)
                    } else {
                        "// HOST_OP: FS_WRITE_ERROR (No Content)".to_string()
                    }
                }
                _ => "// HOST_OP: FS_UNKNOWN".to_string(),
            },
            CodeTaal::SysOp { ref command } => {
                tracing::debug!("[SYS]: SysOp requested '{}'", command);
                "// HOST_OP: SYS_EXEC".to_string()
            }
            CodeTaal::HttpOp {
                ref method,
                ref url,
            } => {
                tracing::debug!("[HTTP]: {} {:?}", method, url);
                "// HOST_OP: HTTP_REQ".to_string()
            }
            CodeTaal::Gebruik { ref path, module_naam: _ } => {
                format!("// WARNING: Unexpanded IMPORT -> {} (Linker skipped?)", path)
            }
            b @ CodeTaal::Block { .. } | b @ CodeTaal::FunctionDef { .. } => {
                tracing::debug!("[SYNTHESIS]: General Block/FunctionDef -> forcing PtxGenerator lowering (bare metal path)");
                let mut ptx_gen = PtxGenerator::new();
                match ptx_gen.lower_general(&b) {
                    Ok(ptx) => ptx,
                    Err(e) => format!("// PTX_GENERAL_LOWER_FAILED: {}", e),
                }
            }
            _ => "// HOST_OP: INTERPRETER_LOGIC (CPU-Side)".to_string(),
        };

        if !cache_key.is_empty() {
            if let Ok(mut cache) = KERNEL_CACHE.lock() {
                cache.insert(cache_key, ptx.clone());
            }
        }
        Ok(ptx)
    }

    /// PTX Synthesizer: Vertalen van de AST GpuKernelDef naar ruwe NVIDIA PTX
    pub fn synthesize_gpu_kernel(kernel: &GpuKernelDef) -> Result<String> {
        let mut generator = PtxGenerator::new();
        generator.generate(kernel)
    }

    /// Convenience for lowered blocks with context (input variables from host scope).
    pub fn synthesize_lowered_with_context(
        code: CodeTaal,
        context: &std::collections::HashMap<String, LiteralValue>,
    ) -> Result<String> {
        let mut generator = PtxGenerator::new();
        generator.lower_general_with_context(&code, context)
    }

    // `translate_ptx_body` is removed as it's now handled by `PtxGenerator`.
    /// Genereert geoptimaliseerde PTX code voor Matrix Mul (Project Godslayer - WMMA Tensor Cores)
    fn generate_matmul_ptx(_m: usize, _n: usize, _k: usize) -> String {
        // WMMA (Warp Matrix Multiply and Accumulate)
        // Uses Hardware Tensor Cores via TF32 precision (allows standard floats as input but uses 19-bit math cores).
        // Warp Size: 32 threads. Block Size: 128 (4 warps).
        r#"
#include <mma.h>
#include <cuda_fp16.h>
using namespace nvcuda;

// Inline PTX for Async Copy
__device__ __forceinline__ void cp_async_16B(void* smem, const void* gmem) {
    unsigned int smem_int = __cvta_generic_to_shared(smem);
    asm volatile("cp.async.cg.shared.global [%0], [%1], 16;\n" :: "r"(smem_int), "l"((const char*)gmem));
}
__device__ __forceinline__ void cp_async_commit() { asm volatile("cp.async.commit_group;\n"); }
__device__ __forceinline__ void cp_async_wait() { asm volatile("cp.async.wait_group 0;\n"); }

// FP16 WMMA Kernel for Maximum Throughput (Project Apex-WMMA)
extern "C" __global__ void matmul_kernel(int M, int N, int K, float alpha, const half* A, const half* B, float beta, float* C) {
    // Block dimension: 256 threads (8 warps).
    // Each block processes a 128x128 tile of C.
    
    // Double buffered shared memory! (2x 128x32)
    __shared__ half s_A[2][128][32]; // K step is 32
    __shared__ half s_B[2][32][128];

    int bx = blockIdx.x;
    int by = blockIdx.y;
    int tx = threadIdx.x;
    
    int warpId = tx / 32;
    int laneId = tx % 32;

    int warpRow = (warpId / 2) * 64;
    int warpCol = (warpId % 2) * 64;

    int globalRow = by * 128 + warpRow;
    int globalCol = bx * 128 + warpCol;

    // 16 accumulators (each 16x16) for the 64x64 tile per warp
    wmma::fragment<wmma::accumulator, 16, 16, 16, float> c_frag[4][4];
    for (int i = 0; i < 4; i++) {
        for (int j = 0; j < 4; j++) {
            wmma::fill_fragment(c_frag[i][j], 0.0f);
        }
    }

    // PRE-LOAD the FIRST tile using cp.async
    // 256 threads loading 128x32 elements = 4096 elements.
    // Each thread loads 16 elements = 2x 16-byte chunks (8 elements each).
    for (int i = 0; i < 2; i++) {
        int a_idx = i * 256 + tx;
        int a_r = a_idx / 4; // 128 rows, 4 chunks of 8 elements = 32 elements.
        int a_c = (a_idx % 4) * 8;
        if ((by * 128 + a_r) < M && a_c < K)
            cp_async_16B(&s_A[0][a_r][a_c], &A[(by * 128 + a_r) * K + a_c]);

        int b_idx = i * 256 + tx;
        int b_r = b_idx / 16; // 32 rows, 16 chunks of 8 elements = 128 elements.
        int b_c = (b_idx % 16) * 8;
        if (b_r < K && (bx * 128 + b_c) < N)
            cp_async_16B(&s_B[0][b_r][b_c], &B[b_r * N + bx * 128 + b_c]);
    }
    cp_async_commit();
    cp_async_wait();
    __syncthreads();

    int load_idx = 1;
    int compute_idx = 0;

    // Iterate over K in chunks of 32
    for (int k_step = 0; k_step < K; k_step += 32) {
        // ASYNC LOAD NEXT TILE
        if (k_step + 32 < K) {
            for (int i = 0; i < 2; i++) {
                int a_idx = i * 256 + tx;
                int a_r = a_idx / 4;
                int a_c = (a_idx % 4) * 8;
                if ((by * 128 + a_r) < M && (k_step + 32 + a_c) < K)
                    cp_async_16B(&s_A[load_idx][a_r][a_c], &A[(by * 128 + a_r) * K + k_step + 32 + a_c]);

                int b_idx = i * 256 + tx;
                int b_r = b_idx / 16;
                int b_c = (b_idx % 16) * 8;
                if ((k_step + 32 + b_r) < K && (bx * 128 + b_c) < N)
                    cp_async_16B(&s_B[load_idx][b_r][b_c], &B[(k_step + 32 + b_r) * N + bx * 128 + b_c]);
            }
            cp_async_commit();
        }

        // COMPUTE CURRENT TILE WITH FP16 TENSOR CORES
        for (int k_frag = 0; k_frag < 32; k_frag += 16) {
            wmma::fragment<wmma::matrix_a, 16, 16, 16, half, wmma::row_major> a_frag[4];
            wmma::fragment<wmma::matrix_b, 16, 16, 16, half, wmma::row_major> b_frag[4];

            #pragma unroll
            for(int i=0; i<4; i++) wmma::load_matrix_sync(a_frag[i], &s_A[compute_idx][warpRow + i*16][k_frag], 32);
            #pragma unroll
            for(int j=0; j<4; j++) wmma::load_matrix_sync(b_frag[j], &s_B[compute_idx][k_frag][warpCol + j*16], 128);

            #pragma unroll
            for (int i = 0; i < 4; i++) {
                #pragma unroll
                for (int j = 0; j < 4; j++) {
                    wmma::mma_sync(c_frag[i][j], a_frag[i], b_frag[j], c_frag[i][j]);
                }
            }
        }

        if (k_step + 32 < K) {
            cp_async_wait();
            __syncthreads();
            load_idx ^= 1;
            compute_idx ^= 1;
        }
    }

    // STORE RESULTS
    for (int i = 0; i < 4; i++) {
        for (int j = 0; j < 4; j++) {
            int r = globalRow + i * 16;
            int c = globalCol + j * 16;
            if (r < M && c < N) {
                wmma::store_matrix_sync(C + r * N + c, c_frag[i][j], N, wmma::mem_row_major);
            }
        }
    }
}
"#.to_string()
    }

    fn generate_tensor_add_ptx(_m: usize, _n: usize) -> String {
        r#"
extern "C" __global__ void tensor_add_kernel(const float* A, const float* B, float* C, int M, int N) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    int total_elements = M * N;
    if (idx < total_elements) {
        C[idx] = A[idx] + B[idx];
    }
}
"#.to_string()
    }

    fn generate_tensor_relu_ptx(_m: usize, _n: usize) -> String {
        r#"
extern "C" __global__ void tensor_relu_kernel(const float* A, float* B, int M, int N) {
    int idx = blockIdx.x * blockDim.x + threadIdx.x;
    int total_elements = M * N;
    if (idx < total_elements) {
        float val = A[idx];
        B[idx] = val > 0.0f ? val : 0.0f;
    }
}
"#.to_string()
    }

    fn generate_vector_add_ptx(_len: usize) -> String {
        r#"
        .version 7.5
        .target sm_75
        .address_size 64
        .visible .entry vector_add_kernel(...) {
            // Vector Add Logic
            ret;
        }
        "#
        .to_string()
    }

    fn generate_chaos_ptx(_intensity: u8, _duration: u64) -> String {
        r#"
        .version 7.5
        .target sm_75
        .address_size 64
        .visible .entry chaos_kernel(...) {
            // Infinite loops and register pressure logic
            ret;
        }
        "#
        .to_string()
    }
}
// ============================================================================
// DYNAMISCHE PTX GENERATOR
// ============================================================================

#[derive(Debug, Clone)]
pub struct PtxGenerator {
    reg_alloc: RegisterAllocator,
    shared_alloc: SharedMemoryAllocator,
    label_counter: usize,
    variables: std::collections::HashMap<String, String>,
    current_line: usize,
    current_col: usize,
}

impl PtxGenerator {
    pub fn new() -> Self {
        Self {
            reg_alloc: RegisterAllocator::new(),
            shared_alloc: SharedMemoryAllocator::new(),
            label_counter: 0,
            variables: std::collections::HashMap::new(),
            current_line: 0,
            current_col: 0,
        }
    }

    pub fn generate(&mut self, kernel: &GpuKernelDef) -> Result<String> {
        let mut ptx = PtxModule::new("sm_80");

        // 1. Entry point + parameters
        ptx.entry_point(&kernel.name);
        for param in &kernel.params {
            let reg = self.reg_alloc.alloc_param(&param.name);
            ptx.add_param(&param.name, &reg);
        }
        ptx.close_entry_point_signature();

        // 2. Shared memory allocaties
        self.shared_alloc.reset();
        
        let ops = if let CodeTaal::Block { statements } = &*kernel.body {
            statements.iter().collect::<Vec<_>>()
        } else {
            vec![&*kernel.body]
        };
        
        for op in ops {
            if let CodeTaal::GpuOp(GpuOperation::MatrixMultiplyAccumulate { .. }) = op {
                self.shared_alloc.allocate_tile("tileA", 512); // 16x16 f16
                self.shared_alloc.allocate_tile("tileB", 256); // 16x8 f16
            }
        }

        // Dynamische register declaraties afdrukken:
        ptx.push(&format!("    .reg .b32 %r<{}>;", self.reg_alloc.max_b32_needed().max(16)));
        ptx.push(&format!("    .reg .b64 %rd<{}>;", self.reg_alloc.max_b64_needed().max(16)));
        ptx.push(&format!("    .reg .f32 %f<{}>;", self.reg_alloc.max_f32_needed().max(32)));
        ptx.push(&format!("    .reg .f16x2 %h<{}>;", self.reg_alloc.max_f16x2_needed().max(32)));
        ptx.push(&format!("    .reg .pred %p<{}>;\n", self.reg_alloc.max_pred_needed().max(16)));

        ptx.add_shared_memory(self.shared_alloc.generate_declarations());

        // Parameter inladen (dit is nog een versimpeling, we linken de namen)
        for (i, param) in kernel.params.iter().enumerate() {
            ptx.push(&format!("    ld.param.u64 %rd{}, [ptr_{}];", i, param.name));
        }
        ptx.push("");

        // 3. Body vertalen
        if let CodeTaal::Block { statements } = &*kernel.body {
            for op in statements {
                self.translate_op(&mut ptx, op)?;
            }
        } else {
            self.translate_op(&mut ptx, &*kernel.body)?;
        }

        // Output store (dummy)
        ptx.push("    st.global.f32 [%rd2], %f0;");

        Ok(ptx.finish())
    }

    /// Lower a general CodeTaal (Block or FunctionDef etc) to PTX using the register allocator.
    /// Supports context binding: external variables from host scope are passed as input params.
    pub fn lower_general(&mut self, code: &CodeTaal) -> Result<String> {
        self.lower_general_with_context(code, &std::collections::HashMap::new())
    }

    /// Full version with context binding for Variable Input Passing.
    /// context: name -> resolved LiteralValue (from host memory scope).
    /// These become .param inputs so that VarGet inside the block can resolve to registers loaded from host.
    pub fn lower_general_with_context(
        &mut self,
        code: &CodeTaal,
        context: &std::collections::HashMap<String, LiteralValue>,
    ) -> Result<String> {
        let mut ptx = PtxModule::new("sm_80");

        // Build the entry signature: result_ptr first, then inputs in sorted order for determinism.
        // For context binding v1 we promote everything to .f32 (ints are converted on load if needed).
        let mut input_params: Vec<String> = context.keys().cloned().collect();
        input_params.sort();

        let mut sig_lines = vec!["    .param .u64 result_ptr".to_string()];
        for name in &input_params {
            sig_lines.push(format!("    .param .f32 input_{}", name));
        }

        ptx.push(".visible .entry hel_lowered (");
        for line in &sig_lines {
            ptx.push(line);
        }
        ptx.push(") {");

        // Declare registers (generous defaults)
        ptx.push(&format!("    .reg .b32 %r<{}>;", self.reg_alloc.max_b32_needed().max(32)));
        ptx.push(&format!("    .reg .b64 %rd<{}>;", self.reg_alloc.max_b64_needed().max(16)));
        ptx.push(&format!("    .reg .f32 %f<{}>;", self.reg_alloc.max_f32_needed().max(32)));
        ptx.push(&format!("    .reg .pred %p<{}>;\n", self.reg_alloc.max_pred_needed().max(16)));

        // Load the result pointer
        ptx.push("    ld.param.u64 %rd1, [result_ptr];");

        // Load context inputs into registers and register them for VarGet.
        // Int and Bool → .b32 integer registers. Floats → f32.
        for name in &input_params {
            let val = context.get(name).ok_or_else(|| anyhow::anyhow!("Ontbrekende variabele in context: {}", name))?;
            match val {
                LiteralValue::Int(_) => {
                    // integers → b32 register
                    let reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                    ptx.push(&format!("    ld.param.b32 {}, [input_{}];", reg, name));
                    self.variables.insert(name.clone(), reg);
                }
                LiteralValue::Float(_) => {
                    let reg = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                    ptx.push(&format!("    ld.param.f32 {}, [input_{}];", reg, name));
                    self.variables.insert(name.clone(), reg);
                }
                LiteralValue::Bool(b) => {
                    // bool → b32 0/1
                    let reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                    ptx.push(&format!("    ld.param.b32 {}, [input_{}];", reg, name));
                    ptx.push(&format!("    // bool {} loaded as b32 mask bit", if *b {1} else {0}));
                    self.variables.insert(name.clone(), reg);
                }
                LiteralValue::List(_) => {
                    // list/matrix → passed as packed b32 by executor before lowering
                    let reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                    ptx.push(&format!("    ld.param.b32 {}, [input_{}];", reg, name));
                    ptx.push(&format!("    // list/matrix loaded as b32 (packed upstream)"));
                    self.variables.insert(name.clone(), reg);
                }
                LiteralValue::Bytes(_) => {
                    // bytes (TCP etc) are host-side only for now; provide dummy b32 param slot
                    let reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                    ptx.push(&format!("    ld.param.b32 {}, [input_{}]; // bytes host-side", reg, name));
                    self.variables.insert(name.clone(), reg);
                }
                LiteralValue::Pointer(_) => {
                    let reg = self.reg_alloc.alloc_b64_fragment(1)[0].clone();
                    ptx.push(&format!("    ld.param.u64 {}, [input_{}]; // pointer host-side", reg, name));
                    self.variables.insert(name.clone(), reg);
                }
                _ => {
                    let reg = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                    ptx.push(&format!("    mov.f32 {}, 0f00000000; // unsupported context type for {}", reg, name));
                    self.variables.insert(name.clone(), reg);
                }
            }
        }

        // Translate the logic (VarGet will now find the input regs)
        self.translate_op(&mut ptx, code)?;

        ptx.push("    ret;");
        ptx.push("}");
        Ok(ptx.finish())
    }

    fn translate_op(&mut self, ptx: &mut PtxModule, op: &CodeTaal) -> Result<()> {
        let res = (|| -> Result<()> {
            match op {
                CodeTaal::LocationMarker { line, col } => {
                    self.current_line = *line;
                    self.current_col = *col;
                    ptx.push(&format!("    // .loc {}:{}", line, col));
                }
                CodeTaal::GpuOp(GpuOperation::SubgroupSync) => {
                    ptx.push("    bar.sync 0;");
                }
                CodeTaal::GpuOp(GpuOperation::MatrixMultiplyAccumulate { a, b, c, m, n, k, precision: _ }) => {
                    self.translate_matrix_multiply(ptx, a, b, c, *m, *n, *k)?;
                }
                CodeTaal::Block { statements } => {
                    for stmt in statements {
                        self.translate_op(ptx, stmt)?;
                    }
                }
                CodeTaal::VarDef { name, value } => {
                    let out_reg = self.translate_expression(ptx, value)?;
                    self.variables.insert(name.clone(), out_reg);
                }
                CodeTaal::Return { value } => {
                    if let Some(val) = value {
                        let out_reg = self.translate_expression(ptx, val)?;
                        if out_reg.starts_with("%r") {
                            // Integer register → b32 store into f32 buffer (bit-cast). CPU reinterprets bits.
                            ptx.push(&format!("    st.global.b32 [%rd1], {};", out_reg));
                        } else {
                            ptx.push(&format!("    st.global.f32 [%rd1], {};", out_reg));
                        }
                    }
                }
                CodeTaal::If { condition, then, else_block } => {
                    let cond_pred = self.translate_expression(ptx, condition)?;
                    let label_end = self.get_label("END_IF");
                    
                    if let Some(else_b) = else_block {
                        let label_else = self.get_label("ELSE");
                        ptx.push(&format!("    @!{} bra {};", cond_pred, label_else));
                        self.translate_op(ptx, then)?;
                        ptx.push(&format!("    bra {};", label_end));
                        ptx.push(&format!("{}:", label_else));
                        self.translate_op(ptx, else_b)?;
                    } else {
                        ptx.push(&format!("    @!{} bra {};", cond_pred, label_end));
                        self.translate_op(ptx, then)?;
                    }
                    ptx.push(&format!("{}:", label_end));
                }
                CodeTaal::Loop { condition, body } => {
                    let label_start = self.get_label("LOOP_START");
                    let label_end = self.get_label("LOOP_END");
                    
                    ptx.push(&format!("{}:", label_start));
                    let cond_pred = self.translate_expression(ptx, condition)?;
                    ptx.push(&format!("    @!{} bra {};", cond_pred, label_end));
                    
                    self.translate_op(ptx, body)?;
                    ptx.push(&format!("    bra {};", label_start));
                    ptx.push(&format!("{}:", label_end));
                }
                _ => {
                    ptx.push(&format!("    // Genegeerd in PTX: {:?}", op));
                }
            }
            Ok(())
        })();

        if let Err(e) = res {
            if self.current_line > 0 {
                return Err(anyhow::anyhow!("PTX JIT Fout op regel {}:{} - {}", self.current_line, self.current_col, e));
            }
            return Err(e);
        }
        Ok(())
    }

    fn translate_expression(&mut self, ptx: &mut PtxModule, op: &CodeTaal) -> Result<String> {
        // Type-aware lowering (plan B): Int -> u32/s32 on %r, Float -> f32 on %f
        match op {
            CodeTaal::MatrixLiteral { rows } => {
                // 2D matrix → packed by executor before lowering
                let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                let r = rows.len();
                let c = rows.first().map_or(0, |row| row.len());
                ptx.push(&format!("    // MatrixLiteral ({}x{}) - use packed context input or global mem", r, c));
                Ok(out_reg)
            }
            CodeTaal::Literal(val) => {
                match val {
                    LiteralValue::Int(i) => {
                        let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                        ptx.push(&format!("    mov.u32 {}, {};", out_reg, i));
                        Ok(out_reg)
                    }
                    LiteralValue::Float(f) => {
                        let out_reg = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                        let f_val = *f as f32;
                        let hex_val = format!("0f{:08x}", f_val.to_bits());
                        ptx.push(&format!("    mov.f32 {}, {}; // {:?}", out_reg, hex_val, val));
                        Ok(out_reg)
                    }
                    LiteralValue::String(s) => {
                        // Strings not directly in PTX registers for compute kernels; treat as host
                        let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                        ptx.push(&format!("    // string literal (host-side) {} -> reg {}", s, out_reg));
                        Ok(out_reg)
                    }
                    LiteralValue::Bool(b) => {
                        let out_reg = self.reg_alloc.alloc_pred();
                        ptx.push(&format!("    setp.ne.u32 {}, 0, {};", out_reg, if *b { 1 } else { 0 }));
                        Ok(out_reg)
                    }
                    LiteralValue::List(_) => {
                        // list/matrix literal — host-side, not directly in PTX scalar reg
                        let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                        ptx.push(&format!("    // list/matrix literal - host/packed side {}", val));
                        Ok(out_reg)
                    }
                    LiteralValue::Bytes(b) => {
                        // Bytes literals are host-side (TCP primitives etc). Do not lower to PTX registers.
                        let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                        ptx.push(&format!("    // bytes literal ({} bytes, host-side only) -> reg {}", b.len(), out_reg));
                        Ok(out_reg)
                    }
                    LiteralValue::Pointer(addr) => {
                        let out_reg = self.reg_alloc.alloc_b64_fragment(1)[0].clone();
                        ptx.push(&format!("    mov.u64 {}, {}; // ptr literal", out_reg, addr));
                        Ok(out_reg)
                    }
                    LiteralValue::Void => {
                        let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                        ptx.push(&format!("    // void literal -> reg {}", out_reg));
                        Ok(out_reg)
                    }
                }
            }
            CodeTaal::VarGet { name } => {
                if let Some(reg) = self.variables.get(name) {
                    Ok(reg.clone())
                } else {
                    Err(anyhow::anyhow!("Variabele '{}' niet gevonden in scope", name))
                }
            }
            CodeTaal::Op { left, op: op_str, right } => {
                let l_reg = self.translate_expression(ptx, left)?;
                let r_reg = self.translate_expression(ptx, right)?;

                // Heuristic type: if regs look like %r (int) use u32 math, else f32
                let is_int_math = l_reg.starts_with("%r") && r_reg.starts_with("%r");

                match op_str.as_str() {
                    "+" | "-" | "*" | "/" | "%" => {
                        if is_int_math {
                            let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                            let ptx_op = match op_str.as_str() {
                                "+" => "add.u32",
                                "-" => "sub.u32",
                                "*" => "mul.lo.u32",
                                "/" => "div.u32",
                                "%" => "rem.u32",
                                _ => unreachable!(),
                            };
                            ptx.push(&format!("    {} {}, {}, {};", ptx_op, out_reg, l_reg, r_reg));
                            Ok(out_reg)
                        } else {
                            let out_reg = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                            let ptx_op = match op_str.as_str() {
                                "+" => "add.f32",
                                "-" => "sub.f32",
                                "*" => "mul.f32",
                                "/" => "div.approx.f32",
                                _ => unreachable!(),
                            };
                            if op_str == "%" {
                                let tmp_div = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                                let tmp_floor = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                                ptx.push(&format!("    div.approx.f32 {}, {}, {};", tmp_div, l_reg, r_reg));
                                ptx.push(&format!("    cvt.rmi.f32.f32 {}, {};", tmp_floor, tmp_div));
                                // l - floor(l/r)*r -> out_reg = -floor(l/r)*r + l
                                // Wait, fma.rn.f32 is a * b + c. We need out = tmp_floor * (-r) + l, but we can't easily negate a register inline.
                                // Let's just do: tmp_mul = tmp_floor * r; sub out, l, tmp_mul
                                let tmp_mul = self.reg_alloc.alloc_f32_fragment(1)[0].clone();
                                ptx.push(&format!("    mul.f32 {}, {}, {};", tmp_mul, tmp_floor, r_reg));
                                ptx.push(&format!("    sub.f32 {}, {}, {};", out_reg, l_reg, tmp_mul));
                            } else {
                                ptx.push(&format!("    {} {}, {}, {};", ptx_op, out_reg, l_reg, r_reg));
                            }
                            Ok(out_reg)
                        }
                    }
                    "&" | "|" | "^" => {
                        // Bitwise ops on b32 integer registers only.
                        if is_int_math {
                            let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                            let ptx_op = match op_str.as_str() {
                                "&" => "and.b32",
                                "|" => "or.b32",
                                "^" => "xor.b32",
                                _ => unreachable!(),
                            };
                            ptx.push(&format!("    {} {}, {}, {};", ptx_op, out_reg, l_reg, r_reg));
                            Ok(out_reg)
                        } else {
                            // fallback or error
                            Err(anyhow::anyhow!("Bitwise ops only supported on integer registers"))
                        }
                    }
                    "<<" | ">>" => {
                        if is_int_math {
                            let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                            let ptx_op = if op_str == "<<" { "shl.b32" } else { "shr.b32" };
                            ptx.push(&format!("    {} {}, {}, {};", ptx_op, out_reg, l_reg, r_reg));
                            Ok(out_reg)
                        } else {
                            Err(anyhow::anyhow!("Shift ops only supported on integer registers"))
                        }
                    }
                    "popc" => {
                        // Population count: popc.b32 on integer register
                        if l_reg.starts_with("%r") {
                            let out_reg = self.reg_alloc.alloc_b32_fragment(1)[0].clone();
                            ptx.push(&format!("    popc.b32 {}, {};", out_reg, l_reg));
                            Ok(out_reg)
                        } else {
                            Err(anyhow::anyhow!("popc only on integer register"))
                        }
                    }
                    "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                        let out_reg = self.reg_alloc.alloc_pred();
                        if is_int_math {
                            let ptx_op = match op_str.as_str() {
                                "==" => "eq", "!=" => "ne", "<" => "lt", ">" => "gt", "<=" => "le", ">=" => "ge", _ => unreachable!(),
                            };
                            ptx.push(&format!("    setp.{}.u32 {}, {}, {};", ptx_op, out_reg, l_reg, r_reg));
                        } else {
                            let ptx_op = match op_str.as_str() {
                                "==" => "eq", "!=" => "ne", "<" => "lt", ">" => "gt", "<=" => "le", ">=" => "ge", _ => unreachable!(),
                            };
                            ptx.push(&format!("    setp.{}.f32 {}, {}, {};", ptx_op, out_reg, l_reg, r_reg));
                        }
                        Ok(out_reg)
                    }
                    _ => Err(anyhow::anyhow!("Onbekende operator: {}", op_str)),
                }
            }
            _ => Err(anyhow::anyhow!("Ongeldige expressie voor GPU: {:?}", op))
        }
    }

    fn translate_matrix_multiply(
        &mut self,
        ptx: &mut PtxModule,
        _a: &str,
        _b: &str,
        _c: &str,
        m: usize,
        n: usize,
        k: usize,
    ) -> Result<()> {
        // Shape selectie
        let shape = match (m, n, k) {
            (16, 8, 16) | (16, _, 16) => "m16n8k16",
            (32, 8, 16) => "m32n8k16",
            _ => "m16n8k16", // fallback
        };

        // Register allocatie voor fragments
        let a_regs = self.reg_alloc.alloc_b32_fragment(4); // A fragment
        let b_regs = self.reg_alloc.alloc_b32_fragment(2); // B fragment
        let d_regs = self.reg_alloc.alloc_f32_fragment(4); // D result

        ptx.push("    mov.u32         %r0, tileA;");
        ptx.push("    mov.u32         %r1, tileB;");

        // LDMATRIX
        ptx.push(&format!(
            "    ldmatrix.sync.aligned.m8n8.x4.shared.b16  {{ {a} }}, [%r0];",
            a = a_regs.join(", ")
        ));

        ptx.push(&format!(
            "    ldmatrix.sync.aligned.m8n8.x2.shared.b16  {{ {b} }}, [%r1];\n",
            b = b_regs.join(", ")
        ));

        // MMA met correcte type signature
        ptx.push(&format!(
            "    mma.sync.aligned.{shape}.row.col.f32.f16.f16.f32\n\
                     {{ {d} }},\n\
                     {{ {a} }},\n\
                     {{ {b} }},\n\
                     {{ {d} }};\n",
            shape = shape,
            d = d_regs.join(", "),
            a = a_regs.join(", "),
            b = b_regs.join(", ")
        ));

        Ok(())
    }

    fn get_label(&mut self, prefix: &str) -> String {
        let l = self.label_counter;
        self.label_counter += 1;
        format!("{}_{}", prefix, l)
    }
}

/// Collect free (external) variable names used in a CodeTaal subtree.
/// A variable is "free" if it is VarGet but not defined by a VarDef in the same scope tree.
/// This is used for context binding when lowering blocks to PTX.
pub fn collect_free_variables(code: &CodeTaal) -> std::collections::HashSet<String> {
    let mut defined = std::collections::HashSet::new();
    let mut used = std::collections::HashSet::new();
    collect_free_vars_impl(code, &mut defined, &mut used);
    used.difference(&defined).cloned().collect()
}

fn collect_free_vars_impl(
    root: &CodeTaal,
    defined: &mut std::collections::HashSet<String>,
    used: &mut std::collections::HashSet<String>,
) {
    let mut stack = vec![root];
    
    while let Some(code) = stack.pop() {
        match code {
            CodeTaal::VarGet { name } => {
                used.insert(name.clone());
            }
            CodeTaal::VarDef { name, value } => {
                defined.insert(name.clone());
                stack.push(&**value);
            }
            CodeTaal::Block { statements } | CodeTaal::Concurrent { statements } => {
                for stmt in statements.iter().rev() {
                    stack.push(stmt);
                }
            }
            CodeTaal::If { condition, then, else_block } => {
                if let Some(e) = else_block {
                    stack.push(&**e);
                }
                stack.push(&**then);
                stack.push(&**condition);
            }
            CodeTaal::Loop { condition, body } => {
                stack.push(&**body);
                stack.push(&**condition);
            }
            CodeTaal::ForEach { iterable, body, .. } => {
                stack.push(&**body);
                stack.push(&**iterable);
            }
            CodeTaal::FunctionDef { body, .. } => {
                stack.push(&**body);
            }
            CodeTaal::Return { value } => {
                if let Some(v) = value {
                    stack.push(&**v);
                }
            }
            CodeTaal::Op { left, right, .. } => {
                stack.push(&**right);
                stack.push(&**left);
            }
            CodeTaal::FileOp { path, content, .. } => {
                if let Some(c) = content {
                    stack.push(&**c);
                }
                stack.push(&**path);
            }
            CodeTaal::HttpOp { url, .. } => {
                stack.push(&**url);
            }
            CodeTaal::TryCatch { try_block, catch_block, .. } => {
                stack.push(&**catch_block);
                stack.push(&**try_block);
            }
            CodeTaal::Daemon { body } => {
                stack.push(&**body);
            }
            CodeTaal::Spawn { body, .. } => {
                stack.push(&**body);
            }
            CodeTaal::Receive { timeout, body, .. } => {
                stack.push(&**body);
                if let Some(t) = timeout {
                    stack.push(&**t);
                }
            }
            CodeTaal::SendMessage { target, message } => {
                stack.push(&**message);
                stack.push(&**target);
            }
            CodeTaal::Handle { body, handlers, .. } => {
                stack.push(&**body);
                for (_, h) in handlers.iter().rev() {
                    stack.push(&**h);
                }
            }
            CodeTaal::Perform { args, .. } => {
                for a in args.iter().rev() {
                    stack.push(a);
                }
            }
            CodeTaal::Resume { continuation, value } => {
                stack.push(&**value);
                stack.push(&**continuation);
            }
            CodeTaal::InlineAssembly { inputs, fallback, .. } => {
                if let Some(f) = fallback {
                    stack.push(&**f);
                }
                for (_, a) in inputs.iter().rev() {
                    stack.push(&**a);
                }
            }
            CodeTaal::TcpListen { addr } => stack.push(&**addr),
            CodeTaal::TcpAccept { listener } => stack.push(&**listener),
            CodeTaal::TcpConnect { addr } => stack.push(&**addr),
            CodeTaal::TcpSend { socket, data } => { stack.push(&**data); stack.push(&**socket); },
            CodeTaal::TcpReceive { socket, max_bytes } => {
                if let Some(m) = max_bytes { stack.push(&**m); }
                stack.push(&**socket);
            },
            CodeTaal::TcpClose { socket } => stack.push(&**socket),
            _ => {}
        }
    }
}

// ==================== HELPERS ====================

#[derive(Debug, Clone)]
struct RegisterAllocator {
    next_b32: u32,
    next_b64: u32,
    next_f32: u32,
    next_f16x2: u32,
    next_pred: u32,
}

impl RegisterAllocator {
    fn new() -> Self {
        Self { next_b32: 2, next_b64: 3, next_f32: 0, next_f16x2: 0, next_pred: 1 } // reserve early regs, p0 could be used sometimes
    }

    fn alloc_param(&mut self, _name: &str) -> String {
        // Dummy implementation for now, just count up
        let start = self.next_b64;
        self.next_b64 += 1;
        format!("%rd{}", start)
    }

    fn alloc_b32_fragment(&mut self, count: usize) -> Vec<String> {
        let start = self.next_b32;
        self.next_b32 += count as u32;
        (start..self.next_b32).map(|i| format!("%r{}", i)).collect()
    }

    fn alloc_b64_fragment(&mut self, count: usize) -> Vec<String> {
        let start = self.next_b64;
        self.next_b64 += count as u32;
        (start..self.next_b64).map(|i| format!("%rd{}", i)).collect()
    }

    fn alloc_f32_fragment(&mut self, count: usize) -> Vec<String> {
        let start = self.next_f32;
        self.next_f32 += count as u32;
        (start..self.next_f32).map(|i| format!("%f{}", i)).collect()
    }

    fn alloc_pred(&mut self) -> String {
        let start = self.next_pred;
        self.next_pred += 1;
        format!("%p{}", start)
    }
    
    fn max_b32_needed(&self) -> u32 { self.next_b32 }
    fn max_b64_needed(&self) -> u32 { self.next_b64 }
    fn max_f32_needed(&self) -> u32 { self.next_f32 }
    fn max_f16x2_needed(&self) -> u32 { self.next_f16x2 }
    fn max_pred_needed(&self) -> u32 { self.next_pred }
}

#[derive(Debug, Clone)]
struct SharedMemoryAllocator {
    tiles: std::collections::HashMap<String, usize>,
}

impl SharedMemoryAllocator {
    fn new() -> Self { Self { tiles: std::collections::HashMap::new() } }
    fn reset(&mut self) { self.tiles.clear(); }

    fn allocate_tile(&mut self, name: &str, bytes: usize) {
        self.tiles.insert(name.to_string(), bytes);
    }



    fn generate_declarations(&self) -> String {
        self.tiles.iter()
            .map(|(name, size)| format!("    .shared .align 16 .b8 {}[{}];", name, size))
            .collect::<Vec<_>>()
            .join("\n") + "\n"
    }
}

// Simpele module builder
struct PtxModule {
    lines: Vec<String>,
}

impl PtxModule {
    fn new(target: &str) -> Self {
        let mut m = Self { lines: vec![] };
        m.lines.push(format!(".version 8.0"));
        m.lines.push(format!(".target {}", target));
        m.lines.push(".address_size 64\n".to_string());
        m
    }

    fn entry_point(&mut self, name: &str) {
        self.lines.push(format!(".visible .entry {} (", name));
    }

    fn add_param(&mut self, name: &str, _reg: &str) {
        self.lines.push(format!("    .param .u64 ptr_{},", name));
    }
    
    fn close_entry_point_signature(&mut self) {
        if let Some(last) = self.lines.last_mut() {
            if last.ends_with(",") {
                last.pop();
            }
        }
        self.lines.push(") {".to_string());
    }

    fn add_shared_memory(&mut self, decls: String) {
        self.lines.push(decls);
    }

    fn push(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    fn finish(&mut self) -> String {
        self.lines.push("    ret;\n}".to_string());
        self.lines.join("\n")
    }


}

// ============================================================================
// GeneralPtxGenerator — bare metal lowering for general CodeTaal
// PTX generation of standard language constructs.
// ============================================================================
pub struct GeneralPtxGenerator {
    next_reg: u32,
    next_pred: u32,
    next_label: u32,
    var_map: std::collections::HashMap<String, String>,
    functions_ptx: String,
    current_line: usize,
    current_col: usize,
}

impl GeneralPtxGenerator {
    pub fn new() -> Self {
        Self {
            next_reg: 0,
            next_pred: 0,
            next_label: 0,
            var_map: std::collections::HashMap::new(),
            functions_ptx: String::new(),
            current_line: 0,
            current_col: 0,
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
            CodeTaal::FunctionDef { name, is_pub: _, params, body } => {
                // Save scope
                let old_map = self.var_map.clone();
                let old_reg = self.next_reg;
                let old_pred = self.next_pred;
                
                self.next_reg = 0;
                self.next_pred = 0;
                self.var_map.clear();

                let mut params_str = Vec::new();
                for p in params.iter() {
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

        let res = (|| -> Result<()> {
            match stmt {
                CodeTaal::LocationMarker { line, col } => {
                    self.current_line = *line;
                    self.current_col = *col;
                    out.push_str(&format!("{}// .loc {}:{}\n", pad, line, col));
                }
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
                CodeTaal::Op { left, op, right } => {
                    if op == "=" {
                        if let CodeTaal::VarGet { name } = &**left {
                            let val_reg = self.translate_expression(out, right, indent)?;
                            if let Some(dst_reg) = self.var_map.get(name) {
                                out.push_str(&format!("{}mov.f64 {}, {};\n", pad, dst_reg, val_reg));
                            } else {
                                let dst_reg = self.alloc_temp_reg();
                                out.push_str(&format!("{}mov.f64 {}, {};\n", pad, dst_reg, val_reg));
                                self.var_map.insert(name.clone(), dst_reg);
                            }
                        }
                    } else {
                        let _ = self.translate_expression(out, stmt, indent)?;
                    }
                }
                CodeTaal::FunctionCall { .. } => {
                    let _ = self.translate_expression(out, stmt, indent)?;
                }
                CodeTaal::If { condition, then, else_block } => {
                    let cond_reg = self.translate_expression(out, condition, indent)?;
                    let p = self.alloc_pred_reg();
                    
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
                    return Err(anyhow::anyhow!("Onondersteund statement voor PTX backend: {:?}", stmt));
                }
            }
            Ok(())
        })();

        if let Err(e) = res {
            if self.current_line > 0 {
                return Err(anyhow::anyhow!("PTX JIT Fout op regel {}:{} - {}", self.current_line, self.current_col, e));
            }
            return Err(e);
        }
        Ok(())
    }

    fn translate_expression(&mut self, out: &mut String, expr: &CodeTaal, indent: usize) -> Result<String> {
        let pad = "    ".repeat(indent);
        match expr {
            CodeTaal::Op { left, op, right } => {
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
                    let logic_op = if op.as_str() == "&&" { "and.pred" } else { "or.pred" };
                    out.push_str(&format!("{}{} {}, {}, {};\n", pad, logic_op, p3, p1, p2));
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
            CodeTaal::Literal(LiteralValue::Bytes(b)) => {
                // Bytes are never emitted into PTX compute kernels directly
                let dst = self.alloc_temp_reg();
                out.push_str(&format!("{}// bytes literal ({} bytes) host-only, dummy reg\n", pad, b.len()));
                out.push_str(&format!("{}mov.u32 {}, 0;\n", pad, dst));
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
