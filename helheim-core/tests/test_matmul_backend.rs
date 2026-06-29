use helheim_core::gpu::cpu_backend::CpuBackend;
use helheim_core::gpu::backend::{GpuBackend, TensorInit};
use helheim_lang::ast::{CodeTaal, GpuKernelDef, GpuParam, GpuType, Precision, GpuOperation, KernelAttribute};

#[test]
fn test_backend_matmul_launch_cpu() {
    let backend = CpuBackend::new();
    
    // Create a dummy GpuKernelDef for MatrixMultiplyAccumulate
    let kernel_def = GpuKernelDef {
        name: "test_matmul_kernel".to_string(),
        attributes: vec![KernelAttribute::WorkgroupSize(256), KernelAttribute::UseTensorCores(true)],
        params: vec![
            GpuParam { name: "a".to_string(), ty: GpuType::Tensor(Precision::F32) },
            GpuParam { name: "b".to_string(), ty: GpuType::Tensor(Precision::F32) },
            GpuParam { name: "c".to_string(), ty: GpuType::Tensor(Precision::F32) },
        ],
        body: Box::new(CodeTaal::Block {
            statements: vec![CodeTaal::GpuOp(GpuOperation::MatrixMultiplyAccumulate {
                a: "a".to_string(),
                b: "b".to_string(),
                c: "c".to_string(),
                m: 1024,
                n: 1024,
                k: 1024,
                precision: Precision::F32,
            })],
        }),
    };

    println!("Compiling kernel on CPU...");
    let compiled = backend.compile(&kernel_def).expect("Compile failed");
    
    let ptr_a = backend.allocate_tensor(&[1024, 1024], Precision::F32, TensorInit::Random).unwrap();
    let ptr_b = backend.allocate_tensor(&[1024, 1024], Precision::F32, TensorInit::Random).unwrap();
    let ptr_c = backend.allocate_tensor(&[1024, 1024], Precision::F32, TensorInit::Zeros).unwrap();

    let start = std::time::Instant::now();
    println!("Launching kernel on CPU (5950X)...");
    backend.launch(&compiled, &[ptr_a, ptr_b, ptr_c]).expect("Launch failed");
    
    backend.synchronize().expect("Sync failed");
    let duration = start.elapsed();
    println!("Test successful! Tijd: {:?}", duration);
}

#[test]
fn test_backend_matmul_launch_ptx() {
    let backend = helheim_core::gpu::get_backend();
    
    // Create a dummy GpuKernelDef for MatrixMultiplyAccumulate
    let kernel_def = GpuKernelDef {
        name: "apex_matmul".to_string(),
        attributes: vec![KernelAttribute::WorkgroupSize(256), KernelAttribute::UseTensorCores(true)],
        params: vec![
            GpuParam { name: "a".to_string(), ty: GpuType::Tensor(Precision::F32) },
            GpuParam { name: "b".to_string(), ty: GpuType::Tensor(Precision::F32) },
            GpuParam { name: "c".to_string(), ty: GpuType::Tensor(Precision::F32) },
        ],
        body: Box::new(CodeTaal::Block {
            statements: vec![CodeTaal::GpuOp(GpuOperation::MatrixMultiplyAccumulate {
                a: "a".to_string(),
                b: "b".to_string(),
                c: "c".to_string(),
                m: 16,
                n: 16,
                k: 16,
                precision: Precision::F32,
            })],
        }),
    };

    println!("Compiling kernel on PTX...");
    let _compiled = backend.compile(&kernel_def).expect("Compile failed");
    println!("PTX Compilation successful!");
}
