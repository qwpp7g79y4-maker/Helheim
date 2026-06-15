#!/bin/bash
echo "🔥 STARTING MAXIMUM TOTAL SYSTEM OVERDRIVE 🔥"
echo "WARNING: This will put 100% load on CPU, RTX 3060, and RTX 5060 Ti."

cd /home/bitboi/dev_2/Helheim

echo "🛠️ Compiling binary..."
cargo build --release --features cuda > /dev/null 2>&1

echo "🚀 [CPU] Launching CPU Burn (Ryzen 5950X - 32 Threads)..."
./target/release/helheim-cli script examples/cpu_burn_max.hel > /dev/null 2>&1 &
CPU_PID=$!

echo "🚀 [GPU 0] Launching MAX GPU Burn on RTX 3060..."
CUDA_VISIBLE_DEVICES=0 ./target/release/helheim-cli script examples/gpu_burn_max.hel > /dev/null 2>&1 &
GPU0_PID=$!

echo "🚀 [GPU 1] Launching MAX GPU Burn on RTX 5060 Ti..."
CUDA_VISIBLE_DEVICES=1 ./target/release/helheim-cli script examples/gpu_burn_max.hel > /dev/null 2>&1 &
GPU1_PID=$!

echo ""
echo "🔥 ALL SYSTEMS MAXED OUT. MONITORING THERMALS FOR 30 SECONDS... 🔥"
echo "Prepare your ears. The 5060 Ti fans should kick in any second."
echo ""

# Monitor via nvidia-smi for 30 seconds
for i in {1..30}
do
    echo "--- Second $i ---"
    nvidia-smi --query-gpu=index,name,temperature.gpu,utilization.gpu,fan.speed,power.draw --format=csv,noheader
    sleep 1
done

echo ""
echo "🛑 THERMAL LIMIT REACHED. KILLING ALL BURN PROCESSES..."
kill $CPU_PID 2>/dev/null
kill $GPU0_PID 2>/dev/null
kill $GPU1_PID 2>/dev/null
echo "✅ SYSTEM RESTORED. HOW WAS THAT?"
