use std::time::Instant;

use serde::{Deserialize, Serialize};

/// HelProbe: Detecteert de kracht van de lokale node.
pub struct HelProbe;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeCapabilities {
    pub has_cuda: bool,
    pub cpu_cores: usize,
    pub estimated_cpu_gflops: f64,
    pub ram_mb: u64,
    pub gpu_count: u32,
    pub gpu_models: Vec<String>,
    pub total_vram: u64,
}

impl HelProbe {
    /// Voer een snelle benchmark uit om de node te profileren
    pub fn probe() -> NodeCapabilities {
        let (gpu_count, gpu_models, total_vram) = Self::get_gpu_info();
        let cpu_cores = num_cpus::get();
        let ram_mb = Self::get_ram_info();
        let estimated_cpu_gflops = Self::benchmark_cpu();
        let has_cuda = gpu_count > 0;

        NodeCapabilities {
            has_cuda,
            cpu_cores,
            estimated_cpu_gflops,
            ram_mb,
            gpu_count,
            gpu_models,
            total_vram,
        }
    }

    fn get_gpu_info() -> (u32, Vec<String>, u64) {
        use std::process::Command;

        // Run nvidia-smi query
        let output = Command::new("nvidia-smi")
            .args([
                "--query-gpu=name,memory.total",
                "--format=csv,noheader,nounits",
            ])
            .output();

        match output {
            Ok(o) if o.status.success() => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let mut models = Vec::new();
                let mut total_mem = 0;

                for line in stdout.lines() {
                    if let Some((name, mem_str)) = line.split_once(',') {
                        models.push(name.trim().to_string());
                        if let Ok(mem) = mem_str.trim().parse::<u64>() {
                            total_mem += mem;
                        }
                    }
                }
                (models.len() as u32, models, total_mem)
            }
            _ => (0, Vec::new(), 0), // No GPU or error
        }
    }

    fn get_ram_info() -> u64 {
        // Simple heuristic: read /proc/meminfo on Linux
        if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
            for line in meminfo.lines() {
                if line.starts_with("MemTotal:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        // Value covers KB, convert to MB
                        if let Ok(kb) = parts[1].parse::<u64>() {
                            return kb / 1024;
                        }
                    }
                }
            }
        }
        4096 // Fallback
    }

    fn benchmark_cpu() -> f64 {
        let start = Instant::now();
        let mut _acc = 1.0f64;
        for i in 1..10_000_000 {
            _acc = _acc.powf(1.0 / (i as f64).sqrt().max(1.1));
        }
        let duration = start.elapsed().as_secs_f64();
        if duration > 0.0 {
            (1.0 / duration) * 10.0
        } else {
            0.0
        }
    }
}
