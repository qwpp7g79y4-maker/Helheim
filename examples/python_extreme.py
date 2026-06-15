import numpy as np
import time

print("=====================================")
print("🐍 PYTHON NUMPY EXTREME BENCHMARK")
print("=====================================")

size = 8192
print(f"1. Allocating massive Matrix A ({size} x {size})...")
A = np.random.rand(size, size).astype(np.float32)

print(f"2. Allocating massive Matrix B ({size} x {size})...")
B = np.random.rand(size, size).astype(np.float32)

print(f"3. Starting CPU Matrix Multiplication (A @ B) - THIS MIGHT TAKE A WHILE...")
start = time.time()
C = A @ B
end = time.time()

elapsed = end - start
# 2 * M * N * K ops
m, n, k = size, size, size
gflops = (2.0 * m * n * k) / (elapsed * 1e9)

print("Done!")
print(f"Time taken: {elapsed:.2f} seconden")
print(f"Performance: {gflops:.2f} GFLOPS")
