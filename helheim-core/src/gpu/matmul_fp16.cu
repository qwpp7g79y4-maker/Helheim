#include <mma.h>
#include <cuda_fp16.h>
using namespace nvcuda;

#define WMMA_M 16
#define WMMA_N 16
#define WMMA_K 16

extern "C" __global__ void matmul_fp16(int m, int n, int k, float alpha, const float* A, const float* B, float beta, float* C) {
    int warpM = (blockIdx.x * blockDim.x + threadIdx.x) / warpSize;
    int warpN = (blockIdx.y * blockDim.y + threadIdx.y);

    int cRow = warpM * WMMA_M;
    int cCol = warpN * WMMA_N;

    if (cRow >= m || cCol >= n) return;

    wmma::fragment<wmma::matrix_a, WMMA_M, WMMA_N, WMMA_K, __half, wmma::row_major> a_frag;
    wmma::fragment<wmma::matrix_b, WMMA_M, WMMA_N, WMMA_K, __half, wmma::col_major> b_frag;
    wmma::fragment<wmma::accumulator, WMMA_M, WMMA_N, WMMA_K, float> acc_frag;
    wmma::fill_fragment(acc_frag, 0.0f);

    __shared__ __half ds_A[WMMA_M][WMMA_K];
    __shared__ __half ds_B[WMMA_K][WMMA_N];

    for (int tile = 0; tile < (k + WMMA_K - 1) / WMMA_K; ++tile) {
        int aCol = tile * WMMA_K;
        int bRow = tile * WMMA_K;

        int laneId = threadIdx.x % warpSize;
        int r = laneId / WMMA_K;
        int c = laneId % WMMA_K;

        if (r < WMMA_M && c < WMMA_K) {
            int ar = cRow + r;
            int ac = aCol + c;
            ds_A[r][c] = (ar < m && ac < k) ? __float2half(A[ar * k + ac]) : __float2half(0.0f);
        }
        if (r < WMMA_K && c < WMMA_N) {
            int br = bRow + r;
            int bc = cCol + c;
            ds_B[r][c] = (br < k && bc < n) ? __float2half(B[br * n + bc]) : __float2half(0.0f);
        }

        __syncwarp();

        wmma::load_matrix_sync(a_frag, &ds_A[0][0], WMMA_K);
        wmma::load_matrix_sync(b_frag, &ds_B[0][0], WMMA_N);
        wmma::mma_sync(acc_frag, a_frag, b_frag, acc_frag);
    }

    wmma::store_matrix_sync(&C[cRow * n + cCol], acc_frag, n, wmma::mem_row_major);
}
