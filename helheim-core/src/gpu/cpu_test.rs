#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::{gpu_alloc_tensor_empty, cpu_execute_matmul, TENSOR_STORE};

    #[test]
    fn test_cpu_fallback_speed() {
        let size = 1024;
        let id_a = gpu_alloc_tensor_empty(size, size).unwrap();
        let id_b = gpu_alloc_tensor_empty(size, size).unwrap();
        let id_c = gpu_alloc_tensor_empty(size, size).unwrap();

        println!("Running CPU-Only MatMul (Rayon i-k-j optimized) for {}x{}...", size, size);
        let gflops = cpu_execute_matmul(id_a, id_b, id_c, size, size, size).unwrap();
        println!("CPU Performance: {:.2} GFLOPS", gflops);
        
        assert!(gflops > 1.0, "CPU should at least be somewhat fast");
    }
}
