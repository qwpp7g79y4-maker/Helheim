import time
import numpy as np
import sys

def benchmark(size):
    print(f"Python/NumPy: Generating {size}x{size} matrices...")
    A = np.random.rand(size, size).astype(np.float32)
    B = np.random.rand(size, size).astype(np.float32)
    
    print(f"Python/NumPy: Starting Matrix Multiplication...")
    start = time.time()
    C = np.dot(A, B)
    duration = time.time() - start
    
    gflops = (2.0 * size**3) / (duration * 1e9)
    print(f"Python/NumPy FINISHED.")
    print(f"Time: {duration:.4f}s")
    print(f"Performance: {gflops:.2f} GFLOPS")

if __name__ == "__main__":
    size = int(sys.argv[1]) if len(sys.argv) > 1 else 4096
    benchmark(size)
