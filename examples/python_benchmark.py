import numpy as np
import time

print("=====================================")
print("   PYTHON NUMPY COMPUTE BENCHMARK")
print("=====================================")

print("1. Allocating massive Matrix A (4096 x 4096)...")
A = np.random.rand(4096, 4096).astype(np.float32)

print("2. Allocating massive Matrix B (4096 x 4096)...")
B = np.random.rand(4096, 4096).astype(np.float32)

print("3. Starting CPU Matrix Multiplication (A @ B)...")
start = time.time()
C = A @ B
end = time.time()

elapsed = end - start
# 2 * M * N * K ops
m, n, k = 4096, 4096, 4096
gflops = (2.0 * m * n * k) / (elapsed * 1e9)

print("Done!")
print(f"Time taken: {elapsed:.2f} seconds")
print(f"Performance: {gflops:.2f} GFLOPS")
