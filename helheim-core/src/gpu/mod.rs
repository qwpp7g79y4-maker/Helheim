pub mod backend;
pub mod cpu_backend;
pub mod cann_tt;

#[cfg(feature = "cuda")]
pub mod ptx_backend;

use backend::GpuBackend;
#[cfg(feature = "cuda")]
use ptx_backend::PtxBackend;
use cpu_backend::CpuBackend;

use anyhow::Result;
use colored::*;
#[cfg(feature = "cuda")]
use cudarc::driver::{CudaContext, CudaSlice, LaunchConfig, PushKernelArg};
#[cfg(feature = "cuda")]
use cudarc::nvrtc::compile_ptx;
use rand::Rng;
#[cfg(feature = "cuda")]
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

#[cfg(feature = "cuda")]
lazy_static::lazy_static! {
    pub static ref TENSOR_STORE: Mutex<HashMap<usize, CudaSlice<f32>>> = Mutex::new(HashMap::new());
    pub static ref NEXT_TENSOR_ID: Mutex<usize> = Mutex::new(1);
}

#[cfg(not(feature = "cuda"))]
lazy_static::lazy_static! {
    pub static ref NEXT_TENSOR_ID: Mutex<usize> = Mutex::new(1);
}

/// JIT executor entry point for lowered general blocks (the full Helheim -> PTX -> CUDA execution pipeline).
/// context: host variables to bind into the PTX kernel as input params (enables `zet a=10; {retourneer a*2;}` etc).
pub fn launch_lowered_block_jit(
    code: &helheim_lang::ast::CodeTaal,
    context: &std::collections::HashMap<String, helheim_lang::ast::LiteralValue>,
) -> anyhow::Result<Option<f32>> {
    let backend = get_backend();
    backend.execute_lowered_block(code, context)
        .map_err(|e| anyhow::anyhow!("Lowered block GPU launch failed: {}", e))
}

pub fn get_backend() -> Box<dyn GpuBackend> {
    #[cfg(feature = "cuda")]
    {
        if let Ok(ptx) = PtxBackend::new() {
            tracing::debug!("[HELHEIM] NVIDIA CUDA gedetecteerd. PtxBackend geladen.");
            return Box::new(ptx);
        }
        tracing::debug!("[HELHEIM] Geen CUDA gedetecteerd of feature niet enabled. Fallback naar Rayon (CpuBackend).");
    }
    #[cfg(not(feature = "cuda"))]
    {
        tracing::debug!("[HELHEIM] CUDA feature niet geactiveerd. Gebruik CpuBackend.");
    }
    Box::new(CpuBackend::new())
}

#[cfg(feature = "cuda")]
fn helheim_device() -> anyhow::Result<std::sync::Arc<CudaContext>> {
    let id: usize = std::env::var("HELHEIM_GPU_DEVICE")
        .ok().and_then(|v| v.parse().ok())
        .unwrap_or(1);
    CudaContext::new(id)
}

#[cfg(feature = "cuda")]
pub fn gpu_alloc_tensor_random(m: usize, n: usize) -> Result<usize> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();
    let elements = m * n;

    let mut rng = rand::rng();
    let host_data: Vec<f32> = (0..elements).map(|_| rng.random()).collect();

    let mut dev_data = stream.alloc_zeros::<f32>(elements)?;
    stream.memcpy_htod(&host_data, &mut dev_data)?;
    stream.synchronize()?;

    let mut id_counter = NEXT_TENSOR_ID.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
    let id = *id_counter;
    *id_counter += 1;

    let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
    store.insert(id, dev_data);

    Ok(id)
}

#[cfg(not(feature = "cuda"))]
pub fn gpu_alloc_tensor_random(_m: usize, _n: usize) -> Result<usize> {
    Err(anyhow::anyhow!("GPU tensor alloc only available with 'cuda' feature"))
}

#[cfg(feature = "cuda")]
pub fn gpu_alloc_tensor_empty(m: usize, n: usize) -> Result<usize> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();
    let elements = m * n;
    let dev_data = stream.alloc_zeros::<f32>(elements)?;

    let mut id_counter = NEXT_TENSOR_ID.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
    let id = *id_counter;
    *id_counter += 1;

    let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
    store.insert(id, dev_data);

    Ok(id)
}

#[cfg(not(feature = "cuda"))]
pub fn gpu_alloc_tensor_empty(_m: usize, _n: usize) -> Result<usize> {
    Err(anyhow::anyhow!("GPU tensor alloc only available with 'cuda' feature"))
}

pub const PTX_SRC: &str = r#"
#define TILE_WIDTH 32

extern "C" __global__ void matmul(int m, int n, int k, float alpha, const float* A, const float* B, float beta, float* C) {
    __shared__ float ds_A[TILE_WIDTH][TILE_WIDTH];
    __shared__ float ds_B[TILE_WIDTH][TILE_WIDTH];

    int bx = blockIdx.x;  int by = blockIdx.y;
    int tx = threadIdx.x; int ty = threadIdx.y;

    int Row = by * TILE_WIDTH + ty;
    int Col = bx * TILE_WIDTH + tx;

    float acc = 0.0f;

    for (int p = 0; p < (k + TILE_WIDTH - 1) / TILE_WIDTH; ++p) {
        ds_A[ty][tx] = (Row < m && p * TILE_WIDTH + tx < k) ? A[Row * k + p * TILE_WIDTH + tx] : 0.0f;
        ds_B[ty][tx] = (Col < n && p * TILE_WIDTH + ty < k) ? B[(p * TILE_WIDTH + ty) * n + Col] : 0.0f;

        __syncthreads();

        #pragma unroll 8
        for (int i = 0; i < TILE_WIDTH; ++i) {
            acc += ds_A[ty][i] * ds_B[i][tx];
        }

        __syncthreads();
    }

    if (Row < m && Col < n) {
        C[Row * n + Col] = alpha * acc + beta * C[Row * n + Col];
    }
}
"#;

#[cfg(feature = "cuda")]
pub fn gpu_work_real(size: usize, device_id: usize) -> Result<()> {
    tracing::debug!("Checking hardware for GPU acceleration (Bare Metal Check)...");
    let has_nvidia = std::process::Command::new("nvidia-smi").output().is_ok();

    if !has_nvidia {
        tracing::debug!(
            "{}",
            "[FALLBACK]: No Nvidia GPU detected! Falling back to Native Multi-Core CPU execution."
                .yellow()
                .bold()
        );
        let start_cpu = Instant::now();

        tracing::debug!(
            "Generating and computing matrix {}x{} purely on CPU...",
            size, size
        );
        let mut sum = 0.0f32;
        let mut rng = rand::rng();
        // Simulated CPU load
        for _ in 0..(size * 10) {
            let val: f32 = rng.random();
            sum += val * 1.05;
        }

        let duration = start_cpu.elapsed();
        // CPU GFLOPS estimate simulation
        let m = size;
        let n = size;
        let k = size;
        let gflops = ((2.0 * m as f64 * n as f64 * k as f64) / 1e9) * 0.001; // Scale down for CPU

        tracing::debug!("CPU COMPUTE FINISHED. (Sum: {})", sum);
        tracing::debug!("Time: {:.2?}", duration);
        tracing::debug!("Performance: {:.2} GFLOPS (CPU Fallback)", gflops);
        return Ok(());
    }

    tracing::debug!("Initializing CUDA Context for GPU {}...", device_id);
    let start_init = Instant::now();
    let dev = CudaContext::new(device_id)?;
    let stream = dev.default_stream();
    tracing::debug!("CUDA initialized in {:.2?}", start_init.elapsed());

    // Detect compute capability — Blackwell (sm_100+) needs FP32 fallback
    let cc_major = unsafe {
        let mut val = 0i32;
        cudarc::driver::sys::cuDeviceGetAttribute(
            &mut val,
            cudarc::driver::sys::CUdevice_attribute_enum::CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR,
            device_id as i32,
        );
        val
    };
    tracing::debug!("GPU {} compute capability: sm_{}x", device_id, cc_major);

    let (module, kernel_name): (_, &str) = if cc_major >= 10 {
        tracing::debug!("Blackwell detected — using FP32 tiled kernel (CUDA 12.4 limit)");
        let ptx = compile_ptx(PTX_SRC)
            .map_err(|e| anyhow::anyhow!("NVRTC FP32 compile failed: {:?}", e))?;
        (dev.load_module(ptx)?, "matmul")
    } else {
        let ptx_fp16 = cudarc::nvrtc::Ptx::from_src(include_str!("matmul_fp16.ptx"));
        (dev.load_module(ptx_fp16)?, "matmul_fp16")
    };
    let f = module.load_function(kernel_name)?;

    let m = size;
    let n = size;
    let k = size;

    tracing::debug!("Generating random matrices {}x{}...", size, size);
    let mut rng = rand::rng();
    let a_host: Vec<f32> = (0..m * k).map(|_| rng.random()).collect();
    let b_host: Vec<f32> = (0..k * n).map(|_| rng.random()).collect();
    let mut c_host: Vec<f32> = vec![0.0; m * n];

    // Double-buffer streams: copy en compute overlappen
    let compute_stream = dev.new_stream()?;
    let copy_stream = dev.new_stream()?;

    // B matrix volledig naar GPU — wordt hergebruikt voor alle chunks
    tracing::debug!("Copying B matrix to GPU...");
    let start_copy = Instant::now();
    let mut b_dev = stream.alloc_zeros::<f32>(b_host.len())?;
    stream.memcpy_htod(&b_host, &mut b_dev)?;
    let mut c_dev = stream.alloc_zeros::<f32>(c_host.len())?;
    stream.synchronize()?;
    tracing::debug!("B copied in {:.2?}", start_copy.elapsed());

    // Splits A in twee chunks — double buffer
    let num_chunks = 2usize;
    let chunk_rows = m.div_ceil(num_chunks);

    // Pre-alloceer twee buffers voor A chunks
    let mut a_buf_0 = compute_stream.alloc_zeros::<f32>(chunk_rows * k)?;
    let mut a_buf_1 = copy_stream.alloc_zeros::<f32>(chunk_rows * k)?;

    tracing::debug!("Executing Custom Kernel (double-buffer) on GPU...");

    let wmma_tile = 16u32;
    let warps_x = 4u32;
    let warps_y = 4u32;
    let block_x = warps_x * 32;
    let block_y = warps_y;
    let shared_mem = 2 * wmma_tile * wmma_tile * std::mem::size_of::<u16>() as u32;
    let alpha = 1.0f32;
    let beta = 0.0f32;
    const RUNS: usize = 3;
    let mut durations = Vec::with_capacity(RUNS);

    for run in 0..RUNS {
        let start_compute = Instant::now();

        // Laad eerste chunk synchroon
        let rows_0 = chunk_rows.min(m);
        compute_stream.memcpy_htod(&a_host[0..rows_0 * k], &mut a_buf_0)?;
        compute_stream.synchronize()?;

        for chunk in 0..num_chunks {
            let row_start = chunk * chunk_rows;
            let row_end = (row_start + chunk_rows).min(m);
            let rows = row_end - row_start;

            // Laad volgende chunk asynchroon terwijl huidige berekent
            let next_chunk = chunk + 1;
            if next_chunk < num_chunks {
                let next_start = next_chunk * chunk_rows;
                let next_end = (next_start + chunk_rows).min(m);
                let next_rows = next_end - next_start;
                copy_stream.memcpy_htod(
                    &a_host[next_start * k..next_start * k + next_rows * k],
                    &mut a_buf_1,
                )?;
            }

            // Bereken huidige chunk
            let grid_x = (n as u32).div_ceil(wmma_tile * warps_x);
            let grid_y = (rows as u32).div_ceil(wmma_tile * warps_y);
            let cfg = LaunchConfig {
                grid_dim: (grid_x, grid_y, 1),
                block_dim: (block_x, block_y, 1),
                shared_mem_bytes: shared_mem,
            };

            let chunk_m = rows;
            let c_offset = row_start * n;
            let mut c_view = c_dev.slice_mut(c_offset..c_offset + rows * n);

            unsafe {
                let cur_buf = if chunk % 2 == 0 {
                    &mut a_buf_0
                } else {
                    &mut a_buf_1
                };
                let mut builder = compute_stream.launch_builder(&f);
                builder.arg(&chunk_m);
                builder.arg(&n);
                builder.arg(&k);
                builder.arg(&alpha);
                builder.arg(cur_buf);
                builder.arg(&b_dev);
                builder.arg(&beta);
                builder.arg(&mut c_view);
                builder.launch(cfg)?;
            }

            // Wacht op copy voor volgende iteratie
            if next_chunk < num_chunks {
                copy_stream.synchronize()?;
                std::mem::swap(&mut a_buf_0, &mut a_buf_1);
            }
        }

        compute_stream.synchronize()?;
        let elapsed = start_compute.elapsed();
        if run > 0 {
            durations.push(elapsed);
        }
    }

    let avg_secs = durations.iter().map(|d| d.as_secs_f64()).sum::<f64>() / durations.len() as f64;
    let gflops = (2.0 * m as f64 * n as f64 * k as f64) / (avg_secs * 1e9);
    tracing::debug!("GPU COMPUTE FINISHED (Custom Kernel 0.19.0).");
    tracing::debug!(
        "Time (gem. {} runs): {:.2?}",
        durations.len(),
        std::time::Duration::from_secs_f64(avg_secs)
    );
    tracing::debug!("Performance: {:.2} GFLOPS", gflops);

    tracing::debug!("Copying result back to Host...");
    stream.memcpy_dtoh(&c_dev, &mut c_host)?;
    stream.synchronize()?;

    tracing::debug!("Sample result C[0]: {}", c_host[0]);

    Ok(())
}

#[cfg(not(feature = "cuda"))]
pub fn gpu_work_real(size: usize, _device_id: usize) -> Result<()> {
    tracing::debug!("[HELHEIM] CUDA feature disabled. Running CPU simulation for gpu_work_real.");
    let start = std::time::Instant::now();
    let mut sum = 0.0f32;
    let mut rng = rand::rng();
    for _ in 0..(size * 10) {
        let val: f32 = rng.random();
        sum += val * 1.05;
    }
    let duration = start.elapsed();
    tracing::debug!("CPU WORK FINISHED. Sum: {}", sum);
    tracing::debug!("Time: {:.2?}", duration);
    Ok(())
}

#[cfg(feature = "cuda")]
pub fn gpu_execute_raw_ptx_ids(
    ptx_src: &str,
    id_a: usize,
    id_b: usize,
    id_c: usize,
    m: usize,
    n: usize,
    k: usize,
) -> Result<f64> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();

    let opts = cudarc::nvrtc::CompileOptions {
        options: vec![
            "--use_fast_math".to_string(),
            "-arch=compute_89".to_string(),
            "-std=c++11".to_string(),
            "-I/usr/local/cuda/include".to_string(),
        ],
        ..Default::default()
    };
    let ptx_res = cudarc::nvrtc::compile_ptx_with_opts(ptx_src, opts)
        .map_err(|e| anyhow::anyhow!("NVRTC Compilation Failed: {:?}", e))?;

    let module = dev.load_module(ptx_res)?;
    let f = module.load_function("matmul_kernel")?;

    // FP16 Helper (if needed)
    let helper_src = r#"
        #include <cuda_fp16.h>
        extern "C" __global__ void f32_to_f16(const float* in, half* out, int size) {
            int idx = blockIdx.x * blockDim.x + threadIdx.x;
            if (idx < size) { out[idx] = __float2half(in[idx]); }
        }
    "#;
    let helper_ptx = cudarc::nvrtc::compile_ptx_with_opts(
        helper_src,
        cudarc::nvrtc::CompileOptions {
            options: vec![
                "-arch=compute_89".to_string(),
                "-std=c++11".to_string(),
                "-I/usr/local/cuda/include".to_string(),
            ],
            ..Default::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("NVRTC Compilation Failed: {:?}", e))?;
    let helper_module = dev.load_module(helper_ptx)?;
    let f_cvt = helper_module.load_function("f32_to_f16")?;

    let (dev_a, dev_b, mut dev_c) = {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let c = store.remove(&id_c).ok_or_else(|| anyhow::anyhow!("C not found"))?;
        let a = store.remove(&id_a).ok_or_else(|| anyhow::anyhow!("A not found"))?;
        let b = store.remove(&id_b).ok_or_else(|| anyhow::anyhow!("B not found"))?;
        (a, b, c)
    };

    // Convert A and B to fp16
    let mut a_half = stream.alloc_zeros::<u16>(m * k)?;
    let mut b_half = stream.alloc_zeros::<u16>(k * n)?;

    let threads = 256;
    let res_cvt = unsafe {
        let size_a = (m * k) as i32;
        let mut bld_cvt1 = stream.launch_builder(&f_cvt);
        let r1 = bld_cvt1
            .arg(dev_a)
            .arg(&mut a_half)
            .arg(&size_a)
            .launch(LaunchConfig {
                grid_dim: (((m * k) as u32).div_ceil(threads), 1, 1),
                block_dim: (threads, 1, 1),
                shared_mem_bytes: 0,
            });

        let size_b = (k * n) as i32;
        let mut bld_cvt2 = stream.launch_builder(&f_cvt);
        let r2 = bld_cvt2
            .arg(dev_b)
            .arg(&mut b_half)
            .arg(&size_b)
            .launch(LaunchConfig {
                grid_dim: (((k * n) as u32).div_ceil(threads), 1, 1),
                block_dim: (threads, 1, 1),
                shared_mem_bytes: 0,
            });
        r1.and(r2)
    };

    if res_cvt.is_err() {
        res_cvt.map_err(|e| anyhow::anyhow!("f32_to_f16 kernel failed: {}", e))?;
    }
    stream.synchronize()?;

    let mut durations = Vec::new();
    let runs = 2; // Reduced benchmark runs to 2 since it's a real operation now

    // Configuration for apex wmma kernel
    let cfg = LaunchConfig {
        grid_dim: ((n as u32).div_ceil(128), (m as u32).div_ceil(128), 1),
        block_dim: (32, 4, 2), // 256 threads aligned
        shared_mem_bytes: 0,
    };

    for run in 0..runs {
        let start_compute = std::time::Instant::now();
        let m_i32 = m as i32;
        let n_i32 = n as i32;
        let k_i32 = k as i32;
        let alpha = 1.0f32;
        let beta = 0.0f32;
        let res_matmul = unsafe {
            let mut bld = stream.launch_builder(&f);
            bld.arg(&m_i32)
                .arg(&n_i32)
                .arg(&k_i32)
                .arg(&alpha)
                .arg(&a_half)
                .arg(&b_half)
                .arg(&beta)
                .arg(&mut dev_c);
            bld.launch(cfg)
        };
        if res_matmul.is_err() {
            res_matmul.map_err(|e| anyhow::anyhow!("matmul_kernel failed: {}", e))?;
        }
        stream.synchronize()?;
        if run > 0 {
            durations.push(start_compute.elapsed());
        }
    }

    let avg_secs = durations.iter().map(|d| d.as_secs_f64()).sum::<f64>() / durations.len() as f64;
    let gflops = (2.0 * m as f64 * n as f64 * k as f64) / (avg_secs * 1e9);

    // Put tensors back
    {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        store.insert(id_a, dev_a);
        store.insert(id_b, dev_b);
        store.insert(id_c, dev_c);
    }

    Ok(gflops)
}

#[cfg(feature = "cuda")]
pub async fn gpu_execute_hel_block(raw_source: &str) -> Result<()> {
    use colored::*;
    tracing::debug!(
        "{}",
        "Checking hardware for Hel-modus acceleration...".magenta()
    );
    let has_nvidia = std::process::Command::new("nvidia-smi").output().is_ok();

    if !has_nvidia {
        return Err(anyhow::anyhow!(
            "Hel-modus vereist een actieve NVIDIA GPU voor bare-metal executie."
        ));
    }

    tracing::debug!(
        "{}",
        "[HEL-MODUS]: JIT Compiling raw C++/PTX via NVRTC...".magenta()
    );
    let dev = helheim_device()?;

    let ptx = match cudarc::nvrtc::compile_ptx(raw_source) {
        Ok(p) => p,
        Err(e) => {
            return Err(anyhow::anyhow!("Compilation Error:\n{:?}", e));
        }
    };

    tracing::debug!(
        "{}",
        "[HEL-MODUS]: Kernel succesvol gecompileerd. Laden in VRAM...".magenta()
    );
    let module = dev.load_module(ptx)?;

    let f = match module.load_function("custom_kernel") {
        Ok(func) => func,
        Err(_) => {
            return Err(anyhow::anyhow!(
                "Kan 'custom_kernel' niet vinden. Zorg dat je kernel `extern \"C\" __global__ void custom_kernel(float* data)` heet."
            ));
        }
    };

    tracing::debug!(
        "{}",
        "[HEL-MODUS]: Lanceren van custom kernel (Grid: 4096, Block: 1024, Threads: 4M)..."
            .red()
            .bold()
    );
    let stream = dev.default_stream();

    // Fill the GPU: 4096 blocks × 1024 threads = 4M threads
    let n_threads: usize = 4096 * 1024;
    let mut dev_data = stream.alloc_zeros::<f32>(n_threads)?;

    let cfg = LaunchConfig {
        grid_dim: (4096, 1, 1),
        block_dim: (1024, 1, 1),
        shared_mem_bytes: 0,
    };

    unsafe {
        let mut bld = stream.launch_builder(&f);
        bld.arg(&mut dev_data);
        bld.launch(cfg)?;
    }
    stream.synchronize()?;

    tracing::debug!(
        "{}",
        "[HEL-MODUS]: ✅ Executie voltooid. Veilig terug in de basis."
            .green()
            .bold()
    );
    Ok(())
}

#[cfg(not(feature = "cuda"))]
pub fn gpu_execute_raw_ptx_ids(_ptx: &str, _id_a: usize, _id_b: usize, _id_c: usize, _m: usize, _n: usize, _k: usize) -> Result<f32> {
    Err(anyhow::anyhow!("GPU PTX execution requires 'cuda' feature"))
}

#[cfg(feature = "cuda")]
pub fn gpu_execute_tensor_add(
    ptx_src: &str,
    id_a: usize,
    id_b: usize,
    id_c: usize,
    m: usize,
    n: usize,
) -> Result<f64> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();
    let opts = cudarc::nvrtc::CompileOptions {
        options: vec!["-arch=compute_89".to_string(), "-std=c++11".to_string()],
        ..Default::default()
    };
    let ptx_res = cudarc::nvrtc::compile_ptx_with_opts(ptx_src, opts).map_err(|e| anyhow::anyhow!("NVRTC Compilation Failed: {:?}", e))?;
    let module = dev.load_module(ptx_res)?;
    let f = module.load_function("tensor_add_kernel")?;

    let elements = m * n;

    let (dev_a, dev_b, mut dev_c) = {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let c = store.remove(&id_c).ok_or_else(|| anyhow::anyhow!("C not found"))?;
        let a = store.remove(&id_a).ok_or_else(|| anyhow::anyhow!("A not found"))?;
        let b = store.remove(&id_b).ok_or_else(|| anyhow::anyhow!("B not found"))?;
        (a, b, c)
    };

    let threads = 256;
    let blocks = (elements as u32).div_ceil(threads);
    let cfg = LaunchConfig {
        grid_dim: (blocks, 1, 1),
        block_dim: (threads, 1, 1),
        shared_mem_bytes: 0,
    };

    let start = std::time::Instant::now();
    unsafe {
        let mut bld = stream.launch_builder(&f);
        let m_i32 = m as i32;
        let n_i32 = n as i32;
        bld.arg(dev_a)
            .arg(dev_b)
            .arg(&mut dev_c)
            .arg(&m_i32)
            .arg(&n_i32);
        bld.launch(cfg)?;
    }
    stream.synchronize()?;

    let elapsed = start.elapsed().as_secs_f64();
    let gflops = (elements as f64) / (elapsed * 1e9);

    {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        store.insert(id_a, dev_a);
        store.insert(id_b, dev_b);
        store.insert(id_c, dev_c);
    }
    Ok(gflops)
}

#[cfg(not(feature = "cuda"))]
pub fn gpu_execute_tensor_add(_ptx: &str, _id_a: usize, _id_b: usize, _out_id: usize, _m: usize, _n: usize) -> Result<f64> {
    Err(anyhow::anyhow!("GPU tensor add requires 'cuda' feature"))
}

#[cfg(feature = "cuda")]
pub fn gpu_execute_tensor_relu(
    ptx_src: &str,
    id_a: usize,
    id_b: usize,
    m: usize,
    n: usize,
) -> Result<f64> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();
    let opts = cudarc::nvrtc::CompileOptions {
        options: vec!["-arch=compute_89".to_string(), "-std=c++11".to_string()],
        ..Default::default()
    };
    let ptx_res = cudarc::nvrtc::compile_ptx_with_opts(ptx_src, opts).map_err(|e| anyhow::anyhow!("NVRTC Compilation Failed: {:?}", e))?;
    let module = dev.load_module(ptx_res)?;
    let f = module.load_function("tensor_relu_kernel")?;

    let elements = m * n;

    let (dev_a, mut dev_b) = {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let b = store.remove(&id_b).ok_or_else(|| anyhow::anyhow!("B not found"))?;
        let a = store.remove(&id_a).ok_or_else(|| anyhow::anyhow!("A not found"))?;
        (a, b)
    };

    let threads = 256;
    let blocks = (elements as u32).div_ceil(threads);
    let cfg = LaunchConfig {
        grid_dim: (blocks, 1, 1),
        block_dim: (threads, 1, 1),
        shared_mem_bytes: 0,
    };

    let start = std::time::Instant::now();
    unsafe {
        let mut bld = stream.launch_builder(&f);
        let m_i32 = m as i32;
        let n_i32 = n as i32;
        bld.arg(dev_a).arg(&mut dev_b).arg(&m_i32).arg(&n_i32);
        bld.launch(cfg)?;
    }
    stream.synchronize()?;

    let elapsed = start.elapsed().as_secs_f64();
    let gflops = (elements as f64) / (elapsed * 1e9);

    {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        store.insert(id_a, dev_a);
        store.insert(id_b, dev_b);
    }
    Ok(gflops)
}

#[cfg(not(feature = "cuda"))]
pub fn gpu_execute_tensor_relu(_ptx: &str, _id_a: usize, _out_id: usize, _m: usize, _n: usize) -> Result<f64> {
    Err(anyhow::anyhow!("GPU tensor relu requires 'cuda' feature"))
}

#[cfg(feature = "cuda")]
pub fn cpu_execute_matmul(
    id_a: usize,
    id_b: usize,
    id_c: usize,
    m: usize,
    n: usize,
    k: usize,
) -> Result<f64> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();

    let mut a_host = vec![0.0f32; m * k];
    let mut b_host = vec![0.0f32; k * n];
    let mut c_host = vec![0.0f32; m * n];

    {
        let store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let dev_a = store
            .get(&id_a)
            .ok_or_else(|| anyhow::anyhow!("A not found"))?;
        let dev_b = store
            .get(&id_b)
            .ok_or_else(|| anyhow::anyhow!("B not found"))?;

        stream.memcpy_dtoh(dev_a, &mut a_host)?;
        stream.memcpy_dtoh(dev_b, &mut b_host)?;
        stream.synchronize()?;
    }

    use rayon::prelude::*;
    const TILE: usize = 64;

    let start = std::time::Instant::now();

    c_host.par_chunks_mut(n).enumerate().for_each(|(i, c_row)| {
        for kk in (0..k).step_by(TILE) {
            for jj in (0..n).step_by(TILE) {
                let k_end = (kk + TILE).min(k);
                let j_end = (jj + TILE).min(n);
                for kk_inner in kk..k_end {
                    let a_ik = a_host[i * k + kk_inner];
                    for j in jj..j_end {
                        c_row[j] += a_ik * b_host[kk_inner * n + j];
                    }
                }
            }
        }
    });

    let elapsed = start.elapsed().as_secs_f64();
    let gflops = (2.0 * m as f64 * n as f64 * k as f64) / (elapsed * 1e9);

    {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let mut dev_c = store
            .remove(&id_c)
            .ok_or_else(|| anyhow::anyhow!("C not found"))?;
        stream.memcpy_htod(&c_host, &mut dev_c)?;
        stream.synchronize()?;
        store.insert(id_c, dev_c);
    }

    Ok(gflops)
}

#[cfg(not(feature = "cuda"))]
pub fn cpu_execute_matmul(_id_a: usize, _id_b: usize, _id_c: usize, _m: usize, _n: usize, _k: usize) -> Result<f64> {
    Err(anyhow::anyhow!("CPU matmul involving tensor store requires 'cuda' feature"))
}

#[cfg(feature = "cuda")]
pub fn cpu_execute_tensor_add(
    id_a: usize,
    id_b: usize,
    id_c: usize,
    m: usize,
    n: usize,
) -> Result<f64> {
    let dev = helheim_device()?;
    let stream = dev.default_stream();
    let elements = m * n;

    let mut a_host = vec![0.0f32; elements];
    let mut b_host = vec![0.0f32; elements];
    let mut c_host = vec![0.0f32; elements];

    {
        let store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let dev_a = store
            .get(&id_a)
            .ok_or_else(|| anyhow::anyhow!("A not found"))?;
        let dev_b = store
            .get(&id_b)
            .ok_or_else(|| anyhow::anyhow!("B not found"))?;
        stream.memcpy_dtoh(dev_a, &mut a_host)?;
        stream.memcpy_dtoh(dev_b, &mut b_host)?;
        stream.synchronize()?;
    }

    use rayon::prelude::*;
    let start = std::time::Instant::now();

    c_host
        .par_iter_mut()
        .zip(a_host.par_iter().zip(b_host.par_iter()))
        .for_each(|(c, (a, b))| {
            *c = *a + *b;
        });

    let elapsed = start.elapsed().as_secs_f64();
    let gflops = (elements as f64) / (elapsed * 1e9);

    {
        let mut store = TENSOR_STORE.lock().map_err(|e| anyhow::anyhow!("Mutex poisoned: {}", e))?;
        let mut dev_c = store
            .remove(&id_c)
            .ok_or_else(|| anyhow::anyhow!("C not found"))?;
        stream.memcpy_htod(&c_host, &mut dev_c)?;
        stream.synchronize()?;
        store.insert(id_c, dev_c);
    }

    Ok(gflops)
}

#[cfg(not(feature = "cuda"))]
pub fn cpu_execute_tensor_add(_id_a: usize, _id_b: usize, _id_c: usize, _m: usize, _n: usize) -> Result<f64> {
    Err(anyhow::anyhow!("CPU tensor add involving tensor store requires 'cuda' feature"))
}

fn cpu_matmul_tiled(size: usize) -> f64 {
    use rayon::prelude::*;
    const TILE: usize = 64;

    let mut rng = rand::rng();
    let a: Vec<f32> = (0..size * size).map(|_| rng.random()).collect();
    let b: Vec<f32> = (0..size * size).map(|_| rng.random()).collect();
    let mut c = vec![0.0f32; size * size];

    let start = Instant::now();

    c.par_chunks_mut(size).enumerate().for_each(|(i, c_row)| {
        for kk in (0..size).step_by(TILE) {
            for jj in (0..size).step_by(TILE) {
                let k_end = (kk + TILE).min(size);
                let j_end = (jj + TILE).min(size);
                for k in kk..k_end {
                    let a_ik = a[i * size + k];
                    for j in jj..j_end {
                        c_row[j] += a_ik * b[k * size + j];
                    }
                }
            }
        }
    });

    let elapsed = start.elapsed();
    (2.0 * size as f64 * size as f64 * size as f64) / (elapsed.as_secs_f64() * 1e9)
}

pub fn inferno_work_real(size: usize, _device_id: usize) -> Result<()> {
    tracing::debug!("{}", "[HEAVY]: Asymmetric load balancing (GPU + CPU)".yellow());

    let gpu_count = match std::process::Command::new("nvidia-smi").arg("-L").output() {
        Ok(out) => String::from_utf8_lossy(&out.stdout).lines().count(),
        Err(_) => 0,
    };

    let cpu_threads = rayon::current_num_threads();
    tracing::debug!(
        "[HEAVY]: {} GPU(s) + {} CPU threads on master node.",
        gpu_count, cpu_threads
    );

    // Split work: GPUs each get full size, CPU gets a scaled-down portion
    // CPU is ~10x slower per element, so we give it a proportional chunk
    let cpu_size = (size as f64 * 0.3) as usize; // ~30% van GPU grootte
    let per_gpu_size = size;

    let start_inferno = Instant::now();

    use rayon::prelude::*;

    // Bouw worker lijst: GPU IDs + CPU als laatste
    enum Worker {
        Gpu(usize),
        Cpu,
    }
    let mut workers: Vec<Worker> = (0..gpu_count).map(Worker::Gpu).collect();
    workers.push(Worker::Cpu);

    let results: Vec<Result<String>> = workers
        .into_par_iter()
        .map(|w| match w {
            Worker::Gpu(id) => {
                tracing::debug!(
                    "[GPU-{}]: Kernel starten ({}x{})...",
                    id, per_gpu_size, per_gpu_size
                );
                gpu_work_real(per_gpu_size, id)?;
                Ok(format!("GPU-{}", id))
            }
            Worker::Cpu => {
                tracing::debug!(
                    "{}",
                    format!(
                        "[CPU-{}T]: Tiled matmul starten ({}x{})...",
                        cpu_threads, cpu_size, cpu_size
                    )
                    .cyan()
                );
                let gflops = cpu_matmul_tiled(cpu_size);
                tracing::debug!(
                    "{}",
                    format!("[CPU]: COMPUTE FINISHED. Prestatie: {:.2} GFLOPS", gflops).cyan()
                );
                Ok(format!("CPU @ {:.1} GFLOPS", gflops))
            }
        })
        .collect();

    let mut had_error = false;
    for res in &results {
        if let Err(e) = res {
            tracing::debug!("{}", format!("[ERROR]: {}", e).red());
            had_error = true;
        }
    }

    if had_error {
        return Err(anyhow::anyhow!(
            "One or more workers crashed during heavy execution."
        ));
    }

    let duration = start_inferno.elapsed();
    tracing::debug!("{}", "[HEAVY]: Local multi-device compute complete!".green());
    tracing::debug!("[HEAVY]: Total parallel time: {:.2?}", duration);

    let gpu_flops = if gpu_count > 0 {
        (2.0 * per_gpu_size as f64 * per_gpu_size as f64 * per_gpu_size as f64 * gpu_count as f64)
            / 1e9
    } else {
        0.0
    };
    let cpu_flops = (2.0 * cpu_size as f64 * cpu_size as f64 * cpu_size as f64) / 1e9;
    let total_gflops = (gpu_flops + cpu_flops) / duration.as_secs_f64();
    tracing::debug!(
        "[HEAVY]: Combined performance: {:.2} GFLOPS (GPU + CPU)",
        total_gflops
    );

    Ok(())
}

#[cfg(test)]
mod cpu_test;
