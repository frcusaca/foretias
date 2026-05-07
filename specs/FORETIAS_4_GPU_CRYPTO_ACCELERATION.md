# Foretias — GPU-Accelerated Crypto Layer (C11 + CUDA)

**Project:** Foretias (Free and Open-source Resilient Time Integrity Attestation Service)
**This document:** Major specification for GPU-accelerated cryptographic primitives in the C11 core layer, targeting high-throughput stamp/verify operations using CUDA.
**Companion documents:**
- `foretias-v1.md` — Foretias v1 product spec
- `FORETIAS_0_OVERVIEW.md` — design invariants, roadmap
- `FORETIAS_1_MVP_SPEC.md` — v0.1 local-server stack (C11 core, Rust node)
- `FORETIAS_2_IMPLEMENTATION_PLAN.md` — v0.2 P2P mutual attestation
- `FORETIAS_3_PQC_INTEGRATION.md` — Post-quantum crypto (SPHINCS+, Dilithium, ML-KEM)

**Target:** AI Coding Specialist for execution. Comments to human reader in parenthesis `(@human ...)`.

**Breaking Changes:** This Major adds new C11 source files and a CUDA compilation path. The C11 core gains an optional GPU backend — all existing crypto paths remain unchanged. The GPU backend is selected via `CryptoServer` trait implementation, not by modifying existing code.

---

## READING ORDER

1. Read `FORETIAS_1_MVP_SPEC.md` — C11 core architecture, Rust FFI pattern
2. Read `FORETIAS_3_PQC_INTEGRATION.md` — SPHINCS+ integration, algorithm-aware CryptoServer trait
3. Scan `p2p/core-engine/src/chronomatter/mod.rs` — stamp/verify call chains
4. Scan `p2p/core-engine/src/foretias/tick.rs` — Foretis struct, stamp(), verify(), verify_pair()
5. Read this document end to end

---

## PART 0 — DESIGN GOALS & GPU TARGET SELECTION

### 0.1 Goal

Add a GPU-accelerated backend to the C11 crypto core that leverages CUDA to accelerate the computationally expensive operations identified in the stamp/verify pipeline: SPHINCS+ signing/verification, SHA-256 batch hashing, and introduce PMAC (Parallel Message Authentication Code) for high-throughput integrity operations.

### 0.2 Why GPU for Crypto

The stamp/verify pipeline has three distinct cost classes:

| Operation | Current Cost | Call Frequency | GPU Benefit |
|-----------|-------------|----------------|-------------|
| SPHINCS+ sign | ~8ms (CPU) | 1 per stamp + 2 per tick | Hypertree paths parallelizable |
| SPHINCS+ verify | ~4ms (CPU) | 1 per verify + 2 per tick pair | WOTS+ verification parallel |
| SHA-256 hash | ~1μs (CPU) | 2 per stamp, 1 per verify | Trivial GPU kernel, 1000x throughput |
| `integrity_check()` loop | N × 8ms | Full calendar (1000s of ticks) | Embarrassingly parallel over tick pairs |

High-throughput nodes (seeding, calendar mirroring, stress testing) perform thousands of stamp/verify operations per second. GPU parallelization targets:
- **Batch stamp/verify** — N independent Foretises processed simultaneously
- **Integrity checking** — thousands of `verify_pair()` calls across calendar
- **SPHINCS+ internal parallelism** — hypertree and WOTS+ components

### 0.3 Why C11 + CUDA (Not Rust GPU Libraries)

- The C11 core (`p2p/core/`) already wraps all crypto libraries (libsodium, liboqs)
- CUDA runtime API (`cuInit`, `cuMemcpy`, `cuLaunchKernel`) is C-compatible
- No Rust GPU dependency needed — GPU kernels launch from C11 via CUDA runtime
- Rust FFI layer calls C11 functions exactly as it does today — zero trait changes
- Existing pattern: `Rust FFI → C11 wrapper → crypto library`. New pattern: `Rust FFI → C11 wrapper → CUDA kernel`

### 0.4 PMAC / Parallel HMAC Rationale

**PMAC (Parallel Message Authentication Code)** — Rogaway's construction:
- Unlike HMAC (sequential block processing), PMAC encrypts each block independently using a block cipher
- Each block's MAC value is computed in parallel → ideal for GPU
- PMAC-1 (single-key) and PMAC2 (dual-key) variants; PMAC2 preferred for GPU (more parallelism)
- NIST SP 800-38C (OMAC) related; PMAC is the parallel variant

**Parallel HMAC** — vectorized HMAC computation:
- Multiple HMAC instances computed simultaneously (different keys/messages)
- Each thread/block handles one HMAC instance
- SHA-256-based HMAC is trivially parallelizable across instances

**PMAC/HMAC in foretias — NOT replacing signatures:**
- SPHINCS+ remains the default signature algorithm for stamp/verify
- PMAC serves as:
  1. **Fast integrity pre-filter** — MAC entire calendar slice before expensive SPHINCS+ verify
  2. **Bulk integrity tagging** — MAC batches of Foretises for mirror transfer validation
  3. **High-throughput attestation chains** — PMAC-linked chains for mutual attestations

### 0.5 Algorithm Selection

| Primitive | Algorithm | GPU Target | Purpose |
|-----------|-----------|------------|---------|
| Hash | SHA-256 | Batch kernel | Content hash, Merkle |
| Signature | SPHINCS+ SHA2-128s | Hypertree/WOTS+ parallel | Stamp signing/verification |
| MAC | PMAC2 (AES-128) | Block-parallel kernel | Integrity pre-filter, bulk tagging |
| MAC | HMAC-SHA256 | Instance-parallel kernel | Bulk integrity, attestation chains |

---

## PART 1 — ARCHITECTURE: GPU CRYPTO BACKEND

### 1.1 Layering

```
┌─────────────────────────────────────────────────────┐
│  Rust (core-engine) — CryptoServer trait             │
│  stamp(), verify(), integrity_check()               │
├─────────────────────────────────────────────────────┤
│  C11 (p2p/core) — GPU crypto wrappers               │
│  foretias_gpu_sign(), foretias_gpu_verify()         │
│  foretias_gpu_batch_sha256(), foretias_pmac_mac()  │
├─────────────────────────────────────────────────────┤
│  CUDA Runtime API (C-compatible)                    │
│  cuInit, cuModuleLoad, cuLaunchKernel, cuMemcpyDtoH│
├─────────────────────────────────────────────────────┤
│  CUDA Kernels (.cu → .cubin)                       │
│  sha256_batch_kernel, sphincs_verify_kernel        │
│  pmac_kernel, hmac_batch_kernel                    │
└─────────────────────────────────────────────────────┘
```

### 1.2 Conditional Compilation

GPU support is optional. CMake detects CUDA availability:

```cmake
find_package(CUDA QUIET)
if(CUDA_FOUND)
    set(FORETIAS_GPU_AVAILABLE 1)
    # Build CUDA kernels, link cuBLAS/cuRAND if needed
else()
    set(FORETIAS_GPU_AVAILABLE 0)
endif()
```

C11 header guard:
```c
#ifdef FORETIAS_GPU_AVAILABLE
// GPU API declarations
#endif
```

### 1.3 GPU CryptoServer Implementation

New Rust implementation of `CryptoServer` trait backed by GPU:

```rust
// p2p/core-engine/src/crypto_server/gpu.rs
pub struct GpuCryptoServer {
    curve: ForetiasCurve,
    pub_key: PublicKeyBytes,
    priv_key: PrivKeyHandle,
    peer_id: ForetiasPeerID,
    seal_key: Zeroizing<[u8; 32]>,
    sphincs_pub_key: SignatureBytes,
    sphincs_secret_key: SignatureBytes,
    dilithium_pub_key: SignatureBytes,
    dilithium_secret_key: SignatureBytes,
    // GPU-specific state
    device_id: i32,
    batch_size: usize,  // max items per GPU batch
}

impl CryptoServer for GpuCryptoServer {
    // Same trait interface as SoftwareCryptoServer
    // GPU-accelerated paths in sign_with(), verify_with()
    // New methods for batch operations (see §1.5)
}
```

### 1.4 Build System Changes

**`p2p/core-engine/build.rs`:**
```rust
// Detect CUDA
let cuda_found = which::which("nvcc").is_ok();

if cuda_found {
    // Pre-compile CUDA kernels to cubin
    for cu_file in ["sha256_batch.cu", "pmac.cu", "hmac_batch.cu"] {
        let out = Command::new("nvcc")
            .args(&["-c", cu_file, "-o", &format!("{}.cubin", cu_file)])
            .output().expect("nvcc failed");
    }
    cc::Build::new()
        .define("FORETIAS_GPU_AVAILABLE", "1")
        // ... rest of C11 sources
} else {
    cc::Build::new()
        .define("FORETIAS_GPU_AVAILABLE", "0")
        // ... rest of C11 sources
}
```

**`p2p/core-engine/Cargo.toml` additions:**
```toml
[target.'cfg(all(target_os = "linux", target_arch = "x86_64"))'.dependencies]
cuda-toolkit = "0.3"  # Optional, for runtime CUDA detection
```

### 1.5 Batch Operations API (C11)

New C11 functions for batch GPU operations:

```c
#ifdef FORETIAS_GPU_AVAILABLE

/* ── Batch SHA-256 ──────────────────────────────────────── */
ForetiasResult foretias_gpu_batch_sha256(
    const uint8_t* inputs[],  // array of input pointers
    const size_t*  lengths,   // array of input lengths
    ForetiasHash32* outputs,  // array of output hashes
    size_t         count      // number of items
);

/* ── Batch SPHINCS+ Verification ────────────────────────── */
ForetiasResult foretias_gpu_batch_sphincs_verify(
    const ForetiasPubKeyVar*  public_keys,  // array of pub keys
    const uint8_t*            messages[],   // array of message pointers
    const size_t*             msg_lengths,  // array of message lengths
    const ForetiasSigVar*     signatures,   // array of signatures
    int*                      results,      // output: 0=valid, -1=invalid
    size_t                    count         // number of items
);

/* ── PMAC2 (AES-128) ────────────────────────────────────── */
ForetiasResult foretias_pmac2_init(
    const uint8_t key[16],    // 128-bit AES key
    void**        ctx_out     // opaque context handle
);

ForetiasResult foretias_pmac2_mac(
    void*        ctx,
    const uint8_t* data,
    size_t       len,
    uint8_t*     mac_out      // 16-byte MAC
);

/* GPU-accelerated PMAC2 — block-parallel */
ForetiasResult foretias_gpu_pmac2_mac(
    void*        ctx,
    const uint8_t* data,
    size_t       len,
    uint8_t*     mac_out      // 16-byte MAC
);

ForetiasResult foretias_pmac2_destroy(void* ctx);

/* ── Batch HMAC-SHA256 ──────────────────────────────────── */
ForetiasResult foretias_gpu_batch_hmac_sha256(
    const uint8_t* keys[],      // array of HMAC keys
    const size_t*  key_lengths, // array of key lengths
    const uint8_t* messages[],  // array of messages
    const size_t*  msg_lengths, // array of message lengths
    ForetiasHash32* macs_out,   // array of HMAC outputs
    size_t         count        // number of items
);

#endif
```

### 1.6 GPU Context Management

GPU resources managed at C11 layer:

```c
// p2p/core/src/gpu_context.h
typedef struct {
    CUdevice       device;
    CUcontext      context;
    CUmodule       sha256_module;
    CUmodule       pmac_module;
    CUmodule       hmac_module;
    CUfunction     sha256_kernel;
    CUfunction     pmac_kernel;
    CUfunction     hmac_kernel;
    int            initialized;
} ForetiasGpuContext;

ForetiasResult foretias_gpu_init(int device_id);
ForetiasResult foretias_gpu_cleanup(void);
int foretias_gpu_available(void);  // returns 1 if CUDA available
int foretias_gpu_device_count(void);
```

---

## PART 2 — CUDA KERNELS

### 2.1 SHA-256 Batch Kernel

Each thread computes one SHA-256 hash. Input data packed into device memory before launch.

```cuda
// p2p/core/src/cuda/sha256_batch.cu

__device__ void sha256_compress(uint32_t state[8], const uint32_t block[16]) {
    // SHA-256 compression function (64 rounds)
    // Standard implementation, optimized for CUDA
}

__global__ void sha256_batch_kernel(
    const uint8_t* __restrict__ data,
    const size_t*  __restrict__ offsets,  // start offset for each item
    const size_t*  __restrict__ lengths,  // length of each item
    uint8_t*       __restrict__ outputs,  // 32 bytes per output
    size_t         count
) {
    size_t idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= count) return;

    size_t offset = offsets[idx];
    size_t len = lengths[idx];
    uint32_t state[8] = { 0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
                           0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19 };

    // Process each 64-byte block
    for (size_t i = 0; i < len; i += 64) {
        uint32_t block[16];
        // Load 64 bytes into 16 uint32_t (big-endian)
        for (int j = 0; j < 16 && (i + j*4) < len; j++) {
            block[j] = ((uint32_t)data[offset + i + j*4] << 24) |
                       ((uint32_t)data[offset + i + j*4 + 1] << 16) |
                       ((uint32_t)data[offset + i + j*4 + 2] << 8) |
                        (uint32_t)data[offset + i + j*4 + 3];
        }
        // Pad if needed
        if (i + 64 > len) {
            // Handle final block padding
        }
        sha256_compress(state, block);
    }

    // Finalization (padding + length)
    // Write 32-byte output
    for (int i = 0; i < 8; i++) {
        outputs[idx * 32 + i*4]     = (state[i] >> 24) & 0xFF;
        outputs[idx * 32 + i*4 + 1] = (state[i] >> 16) & 0xFF;
        outputs[idx * 32 + i*4 + 2] = (state[i] >>  8) & 0xFF;
        outputs[idx * 32 + i*4 + 3] =  state[i]        & 0xFF;
    }
}
```

**Launch parameters:**
- Block size: 256 threads
- Grid size: ceil(count / 256) blocks
- Memory: all input data packed contiguously + offset/length arrays

### 2.2 PMAC2 Kernel

PMAC2 processes each 16-byte block independently using AES encryption:

```cuda
// p2p/core/src/cuda/pmac.cu

__device__ void aes128_encrypt(const uint8_t key[16], const uint8_t pt[16], uint8_t ct[16]) {
    // AES-128 encryption (10 rounds)
    // NVIDIA provides optimized implementations in libcu++, but standalone version here
}

__device__ void pmac2_pad(const uint8_t key1[16], const uint8_t key2[16],
                          const uint8_t* block, uint8_t padded[16], size_t remaining) {
    // PMAC2 padding: if partial block, pad with 1000...0 or 0000...01
    // XOR with L1 or L2 depending on padding type
}

__global__ void pmac2_kernel(
    const uint8_t* __restrict__ data,
    const size_t   __restrict__ offsets,
    const size_t   __restrict__ lengths,
    const uint8_t* __restrict__ aes_key,
    const uint8_t* __restrict__ l1,  // derived subkey 1
    const uint8_t* __restrict__ l2,  // derived subkey 2
    uint8_t*       __restrict__ macs_out,
    size_t         count
) {
    size_t idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= count) return;

    size_t offset = offsets[idx];
    size_t len = lengths[idx];

    uint8_t sum1[16] = {0};  // PMAC2 accumulator 1
    uint8_t sum2[16] = {0};  // PMAC2 accumulator 2

    // Process complete 16-byte blocks in pairs
    size_t full_blocks = len / 16;
    for (size_t i = 0; i < full_blocks; i += 2) {
        uint8_t ct1[16], ct2[16];
        aes128_encrypt(aes_key, &data[offset + i*16], ct1);
        if (i + 1 < full_blocks) {
            aes128_encrypt(aes_key, &data[offset + (i+1)*16], ct2);
        } else {
            // Odd block — use padding
            pmac2_pad(l1, l2, &data[offset + (i+1)*16], ct2, 0);
        }
        xor16(sum1, sum1, ct1);
        xor16(sum2, sum2, ct2);
    }

    // Handle remaining partial block
    size_t remaining = len % 16;
    if (remaining > 0) {
        uint8_t padded[16];
        memcpy(padded, &data[offset + full_blocks*16], remaining);
        pmac2_pad(l1, l2, padded, padded, remaining);
        uint8_t ct[16];
        aes128_encrypt(aes_key, padded, ct);
        xor16(sum2, sum2, ct);
    }

    // Final: mac = Encrypt(key, sum1 XOR sum2)
    uint8_t final[16];
    xor16(final, sum1, sum2);
    aes128_encrypt(aes_key, final, macs_out + idx * 16);
}
```

### 2.3 HMAC-SHA256 Batch Kernel

Each thread computes one HMAC-SHA256 instance. Inner hash (key ⊕ ipad || message), outer hash (key ⊕ opad || inner_hash).

```cuda
// p2p/core/src/cuda/hmac_batch.cu

__global__ void hmac_sha256_batch_kernel(
    const uint8_t* __restrict__ keys,
    const size_t*  __restrict__ key_lengths,
    const uint8_t* __restrict__ messages,
    const size_t*  __restrict__ msg_offsets,
    const size_t*  __restrict__ msg_lengths,
    uint8_t*       __restrict__ macs_out,
    size_t         count
) {
    size_t idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= count) return;

    // Derive inner/outer padded keys (64 bytes for SHA-256)
    uint8_t inner_key[64] = {0x36};
    uint8_t outer_key[64] = {0x5c};

    size_t key_len = key_lengths[idx];
    if (key_len > 64) {
        // Hash the key first
        key_len = 32;  // Use SHA-256 of key
    }
    memcpy(inner_key, keys + idx * 64, key_len);
    memcpy(outer_key, keys + idx * 64, key_len);

    // Inner hash: SHA-256(inner_key || message)
    uint32_t inner_state[8] = sha256_iv;
    sha256_update(inner_state, inner_key, 64);
    sha256_update(inner_state, messages + msg_offsets[idx], msg_lengths[idx]);
    uint8_t inner_hash[32];
    sha256_finalize(inner_state, inner_hash);

    // Outer hash: SHA-256(outer_key || inner_hash)
    uint32_t outer_state[8] = sha256_iv;
    sha256_update(outer_state, outer_key, 64);
    sha256_update(outer_state, inner_hash, 32);
    sha256_finalize(outer_state, macs_out + idx * 32);
}
```

### 2.4 SPHINCS+ Hypertree Parallel Kernel

SPHINCS+ signing requires computing WOTS+ signatures and hypertree path hashes. The hypertree is a binary tree where each leaf is a WOTS+ public key, and internal nodes are SHA-256 hashes of concatenated children.

**GPU strategy:**
1. Compute all WOTS+ chains in parallel (one thread per chain)
2. Compute hypertree bottom-up using parallel tree reduction
3. Layer hashes computed in parallel

```cuda
// p2p/core/src/cuda/sphincs_hypertree.cu

// WOTS+ chain computation — each thread computes one chain
__global__ void wots_chain_kernel(
    const uint8_t* __restrict__ seed,      // WOTS+ seed
    const uint8_t* __restrict__ msg,       // message to sign
    uint8_t*       __restrict__ chains,    // output chains
    size_t         chain_count,            // number of chains
    size_t         chain_length,           // WOTS+ parameter (e.g., 10)
    uint32_t       wots_address,           // WOTS+ address
    uint32_t       layer,                  // layer address
    uint32_t       tree,                   // tree address
    uint32_t       subtree                 // subtree address
) {
    size_t idx = blockIdx.x * blockDim.x + threadIdx.x;
    if (idx >= chain_count) return;

    uint8_t chain_state[32];  // current hash state
    // Initialize from seed + address
    shake256_init(chain_state, seed, wots_address, idx, layer, tree, subtree);

    // Compute chain[msg[idx]] iterations of SHAKE-256
    uint8_t msg_byte = msg[idx];
    for (uint8_t i = 0; i < msg_byte; i++) {
        shake256_update(chain_state, chain_state, 32);
    }

    memcpy(chains + idx * 32, chain_state, 32);
}

// Hypertree bottom-up hash — parallel tree reduction
__global__ void hypertree_hash_kernel(
    const uint8_t* __restrict__ leaves,   // leaf hashes (WOTS+ pub keys)
    uint8_t*       __restrict__ tree,     // output tree nodes
    size_t         leaf_count             // number of leaves (power of 2)
) {
    // Each level: thread pair hashes two children into one parent
    // Done level by level with synchronization
    extern __shared__ uint8_t shared_node[64];  // 2 × 32-byte hashes

    size_t tid = threadIdx.x;
    size_t gid = blockIdx.x * blockDim.x + threadIdx.x;

    // Load leaves at current level
    if (gid < leaf_count) {
        memcpy(shared_node + tid * 32, leaves + gid * 32, 32);
    }
    __syncthreads();

    // Bottom-up reduction
    for (size_t stride = leaf_count / 2; stride > 0; stride >>= 1) {
        if (tid < stride) {
            uint8_t child0[32], child1[32];
            memcpy(child0, shared_node + tid * 64, 32);
            memcpy(child1, shared_node + (tid + stride) * 32, 32);
            // Hash concatenation
            sha256_concat(child0, child1, shared_node + tid * 32);
        }
        __syncthreads();
    }

    // Write root hash
    if (tid == 0) {
        memcpy(tree + blockIdx.x * 32, shared_node, 32);
    }
}
```

---

## PART 3 — C11 GPU WRAPPER FILES

### 3.1 File Layout

| File | Purpose |
|------|---------|
| `p2p/core/src/gpu_context.c` | GPU context initialization, CUDA runtime API wrappers |
| `p2p/core/src/gpu_context.h` | Internal header for GPU context |
| `p2p/core/src/gpu_hash.c` | Batch SHA-256 GPU wrapper |
| `p2p/core/src/gpu_sphincs.c` | Batch SPHINCS+ verification GPU wrapper |
| `p2p/core/src/pmac.c` | PMAC2 implementation (CPU fallback + GPU) |
| `p2p/core/src/hmac_batch.c` | Batch HMAC-SHA256 GPU wrapper |
| `p2p/core/src/cuda/sha256_batch.cu` | SHA-256 batch CUDA kernel |
| `p2p/core/src/cuda/pmac.cu` | PMAC2 CUDA kernel |
| `p2p/core/src/cuda/hmac_batch.cu` | HMAC-SHA256 batch CUDA kernel |
| `p2p/core/src/cuda/sphincs_hypertree.cu` | SPHINCS+ hypertree CUDA kernel |

### 3.2 gpu_context.c — CUDA Runtime Wrappers

```c
// p2p/core/src/gpu_context.c
#include "foretias_core.h"
#include "gpu_context.h"

#ifdef FORETIAS_GPU_AVAILABLE
#include <cuda_runtime.h>
#include <driver_types.h>

static ForetiasGpuContext gpu_ctx = {0};

ForetiasResult foretias_gpu_init(int device_id) {
    if (gpu_ctx.initialized) return FORETIAS_OK;

    CUresult err;
    err = cuInit(0);
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_INTERNAL;

    err = cuDeviceGet(&gpu_ctx.device, device_id);
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_UNSUPPORTED;

    err = cuCtxCreate(&gpu_ctx.context, 0, gpu_ctx.device);
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_INTERNAL;

    // Load CUDA modules (pre-compiled .cubin files)
    err = cuModuleLoad(&gpu_ctx.sha256_module, "sha256_batch.cubin");
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_INTERNAL;

    err = cuModuleLoad(&gpu_ctx.pmac_module, "pmac.cubin");
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_INTERNAL;

    err = cuModuleLoad(&gpu_ctx.hmac_module, "hmac_batch.cubin");
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_INTERNAL;

    err = cuModuleGetFunction(&gpu_ctx.sha256_kernel, gpu_ctx.sha256_module, "sha256_batch_kernel");
    if (err != CUDA_SUCCESS) return FORETIAS_ERR_INTERNAL;

    gpu_ctx.initialized = 1;
    return FORETIAS_OK;
}

ForetiasResult foretias_gpu_cleanup(void) {
    if (!gpu_ctx.initialized) return FORETIAS_OK;

    cuModuleUnload(gpu_ctx.sha256_module);
    cuModuleUnload(gpu_ctx.pmac_module);
    cuModuleUnload(gpu_ctx.hmac_module);
    cuCtxDestroy(gpu_ctx.context);
    gpu_ctx.initialized = 0;
    return FORETIAS_OK;
}

int foretias_gpu_available(void) {
    int device_count = 0;
    return cuInit(0) == CUDA_SUCCESS &&
           cuDeviceGetCount(&device_count) == CUDA_SUCCESS &&
           device_count > 0;
}
#endif
```

### 3.3 pmac.c — PMAC2 Implementation

```c
// p2p/core/src/pmac.c
#include "foretias_core.h"
#include <string.h>

typedef struct {
    uint8_t key[16];
    uint8_t l1[16];  // L1 = AES(key, 0) * 2
    uint8_t l2[16];  // L2 = L1 * 2
} ForetiasPmac2Ctx;

// AES-128 encryption (uses libsodium crypto_aes128 or standalone)
static void aes128_ecb(const uint8_t key[16], const uint8_t pt[16], uint8_t ct[16]);

static void derive_pmac2_keys(const uint8_t key[16], uint8_t l1[16], uint8_t l2[16]) {
    uint8_t zero[16] = {0};
    uint8_t e0[16];
    aes128_ecb(key, zero, e0);
    // L1 = e0 << 1 (with μ polynomial correction)
    pmac_shift_left(e0, l1);
    // L2 = L1 << 1
    pmac_shift_left(l1, l2);
}

ForetiasResult foretias_pmac2_init(const uint8_t key[16], void** ctx_out) {
    ForetiasPmac2Ctx* ctx = malloc(sizeof(ForetiasPmac2Ctx));
    if (!ctx) return FORETIAS_ERR_INTERNAL;
    memcpy(ctx->key, key, 16);
    derive_pmac2_keys(key, ctx->l1, ctx->l2);
    *ctx_out = ctx;
    return FORETIAS_OK;
}

ForetiasResult foretias_pmac2_mac(void* ctx, const uint8_t* data, size_t len, uint8_t* mac_out) {
    ForetiasPmac2Ctx* p = (ForetiasPmac2Ctx*)ctx;

#ifdef FORETIAS_GPU_AVAILABLE
    // GPU path for large data
    if (len > 1024) {
        return foretias_gpu_pmac2_mac(ctx, data, len, mac_out);
    }
#endif

    // CPU path — process blocks
    uint8_t sum1[16] = {0};
    uint8_t sum2[16] = {0};

    size_t full_blocks = len / 16;
    for (size_t i = 0; i < full_blocks; i += 2) {
        uint8_t ct1[16], ct2[16];
        aes128_ecb(p->key, data + i*16, ct1);
        if (i + 1 < full_blocks) {
            aes128_ecb(p->key, data + (i+1)*16, ct2);
        } else {
            // Odd block padding
            memcpy(ct2, data + (i+1)*16, 16);
            ct2[15] ^= 0x01;
            aes128_ecb(p->key, ct2, ct2);
            // XOR with L2
            xor16(ct2, ct2, p->l2);
        }
        xor16(sum1, sum1, ct1);
        xor16(sum2, sum2, ct2);
    }

    // Remaining partial block
    size_t remaining = len % 16;
    if (remaining > 0) {
        uint8_t padded[16] = {0};
        memcpy(padded, data + full_blocks*16, remaining);
        padded[remaining] = 0x01;  // PMAC2 padding
        uint8_t ct[16];
        aes128_ecb(p->key, padded, ct);
        xor16(sum2, sum2, ct);
    }

    // Final encryption
    uint8_t final[16];
    xor16(final, sum1, sum2);
    aes128_ecb(p->key, final, mac_out);
    return FORETIAS_OK;
}
```

---

## PART 4 — RUST GPU WRAPPER

### 4.1 GpuCryptoServer

```rust
// p2p/core-engine/src/crypto_server/gpu.rs
use super::{CryptoServer, CryptoServerCapabilities, ForetiasCurve, PublicKeyBytes, SharedSecret, SealedBlob};
use crate::core::bindings::*;
use crate::core::identity::PrivKeyHandle;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::{SignatureAlgorithm, SignatureBytes};

pub struct GpuCryptoServer {
    curve: ForetiasCurve,
    pub_key: PublicKeyBytes,
    priv_key: PrivKeyHandle,
    peer_id: ForetiasPeerID,
    sphincs_pub_key: SignatureBytes,
    sphincs_secret_key: SignatureBytes,
    device_id: i32,
}

impl GpuCryptoServer {
    pub fn generate(curve: ForetiasCurve, device_id: i32) -> Result<Self, CryptoError> {
        // Initialize GPU context
        let rc = unsafe { foretias_gpu_init(device_id) };
        c_result_to_error(rc)?;

        // Generate keys (same as SoftwareCryptoServer)
        PrivKeyHandle::init();
        let handle = PrivKeyHandle::generate()?;
        let pub_key_bytes: [u8; 32] = handle.public_key()?;
        let pub_key = ForetiasPubKey32 { bytes: pub_key_bytes };
        let peer_id = crate::core::identity::derive_ed25519_peer_id(&pub_key)?;

        let sphincs_keys = super::signing_sphincs::sphincs_keypair()?;

        Ok(Self {
            curve,
            pub_key: PublicKeyBytes::Ed25519(pub_key),
            priv_key: handle,
            peer_id,
            sphincs_pub_key: sphincs_keys.0,
            sphincs_secret_key: sphincs_keys.1,
            device_id,
        })
    }
}

impl Drop for GpuCryptoServer {
    fn drop(&mut self) {
        unsafe { foretias_gpu_cleanup(); }
    }
}

impl CryptoServer for GpuCryptoServer {
    fn public_key(&self) -> PublicKeyBytes { self.pub_key }
    fn peer_id(&self) -> ForetiasPeerID { self.peer_id }
    fn curve(&self) -> ForetiasCurve { self.curve }

    fn capabilities(&self) -> CryptoServerCapabilities {
        CryptoServerCapabilities {
            backend_name: "gpu",
            curve: self.curve,
            supports_proof: false,
            supports_sealing: true,
            max_sealed_bytes: 1_048_576,
            typical_sign_us: 500,  // GPU SPHINCS+ ~500μs (batch)
            typical_ecdh_us: 50,
        }
    }

    // ... standard methods same as SoftwareCryptoServer ...

    fn signature_algorithm(&self) -> SignatureAlgorithm {
        SignatureAlgorithm::SPHINCS_SHA2_128S
    }

    fn sign_with(&self, msg: &[u8], alg: SignatureAlgorithm) -> Result<SignatureBytes, CryptoError> {
        // GPU-accelerated SPHINCS+ signing
        match alg {
            SignatureAlgorithm::SPHINCS_SHA2_128S => {
                super::signing_sphincs::sphincs_sign(&self.sphincs_secret_key, msg)
            }
            _ => Err(CryptoError::Unsupported("GPU sign: algorithm not supported")),
        }
    }

    fn verify_with(&self, pub_key: &SignatureBytes, alg_id: &str, msg: &[u8], sig: &SignatureBytes)
        -> Result<bool, CryptoError>
    {
        match SignatureAlgorithm::from_id_string(alg_id)? {
            SignatureAlgorithm::SPHINCS_SHA2_128S => {
                super::signing_sphincs::sphincs_verify(pub_key, msg, sig)
            }
            SignatureAlgorithm::Ed25519 => {
                // CPU path for Ed25519 (fast enough)
                let pk_bytes: [u8; 32] = pub_key[..32].try_into().map_err(|_| CryptoError::BadKey)?;
                let sig_bytes: [u8; 64] = sig[..64].try_into().map_err(|_| CryptoError::BadSignature)?;
                self.verify_ed25519(&ForetiasPubKey32 { bytes: pk_bytes }, msg, &ForetiasSig64 { bytes: sig_bytes })
            }
            _ => Err(CryptoError::Unsupported("GPU verify: algorithm not supported")),
        }
    }
}
```

### 4.2 BatchCryptoEngine — Unified GPU / Rayon Dispatch

The `BatchCryptoEngine` abstracts over GPU and CPU parallel backends. Callers invoke batch operations without knowing which backend executes them. GPU is preferred when available; rayon multi-threading is the fallback.

```rust
// p2p/core-engine/src/crypto_server/batch_engine.rs

use crate::core::bindings::*;
use crate::error::{c_result_to_error, CryptoError};
use crate::foretias::types::{SignatureAlgorithm, SignatureBytes};
use rayon::prelude::*;

/// Auto-detect backend capability at construction time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchBackend {
    /// CUDA GPU available — uses GPU kernels
    Gpu { device_id: i32 },
    /// No GPU — uses rayon parallel threads on CPU
    Rayon { max_threads: usize },
}

impl BatchBackend {
    /// Detect best available backend.
    pub fn detect() -> Self {
        #[cfg(feature = "gpu")]
        {
            if unsafe { foretias_gpu_available() } != 0 {
                return Self::Gpu { device_id: 0 };
            }
        }
        let threads = num_cpus::get().min(32);
        Self::Rayon { max_threads: threads }
    }
}

/// Unified batch crypto engine — dispatches to GPU or rayon.
pub struct BatchCryptoEngine {
    backend: BatchBackend,
    gpu_ctx_initialized: std::sync::OnceLock<()>,
}

impl BatchCryptoEngine {
    pub fn new(backend: BatchBackend) -> Self {
        Self {
            backend,
            gpu_ctx_initialized: std::sync::OnceLock::new(),
        }
    }

    pub fn auto() -> Self {
        Self::new(BatchBackend::detect())
    }

    fn ensure_gpu(&self) -> Result<(), CryptoError> {
        if let BatchBackend::Gpu { device_id } = self.backend {
            self.gpu_ctx_initialized.get_or_try_init(|| {
                let rc = unsafe { foretias_gpu_init(*device_id) };
                c_result_to_error(rc)?;
                Ok(())
            })?;
        }
        Ok(())
    }

    /// Returns true if GPU backend is active.
    pub fn is_gpu(&self) -> bool {
        matches!(self.backend, BatchBackend::Gpu { .. })
    }
}

// ── Batch SHA-256 ──────────────────────────────────────────

impl BatchCryptoEngine {
    /// Batch SHA-256 — GPU or rayon dispatch.
    /// GPU threshold: >= 64 items. Below that, rayon is faster.
    pub fn batch_sha256(&self, data: &[&[u8]]) -> Result<Vec<ForetiasHash32>, CryptoError> {
        match self.backend {
            BatchBackend::Gpu { .. } if data.len() >= 64 => self.gpu_batch_sha256(data),
            _ => self.rayon_batch_sha256(data),
        }
    }

    fn gpu_batch_sha256(&self, data: &[&[u8]]) -> Result<Vec<ForetiasHash32>, CryptoError> {
        self.ensure_gpu()?;
        let count = data.len();
        let mut outputs = vec![unsafe { std::mem::zeroed() }; count];
        let inputs: Vec<*const u8> = data.iter().map(|d| d.as_ptr()).collect();
        let lengths: Vec<size_t> = data.iter().map(|d| d.len()).collect();
        let rc = unsafe {
            foretias_gpu_batch_sha256(
                inputs.as_ptr(), lengths.as_ptr(),
                outputs.as_mut_ptr(), count,
            )
        };
        c_result_to_error(rc)?;
        Ok(outputs)
    }

    fn rayon_batch_sha256(&self, data: &[&[u8]]) -> Result<Vec<ForetiasHash32>, CryptoError> {
        data.par_iter()
            .map(|d| {
                let mut out = unsafe { std::mem::zeroed() };
                let rc = unsafe { foretias_hash_sha256(d.as_ptr(), d.len(), &mut out) };
                c_result_to_error(rc).map(|_| out)
            })
            .collect()
    }
}

// ── Batch SPHINCS+ Verification ────────────────────────────

impl BatchCryptoEngine {
    /// Batch SPHINCS+ verification — GPU or rayon dispatch.
    /// GPU threshold: >= 16 items. Below that, rayon is faster.
    pub fn batch_verify(
        &self,
        pub_keys: &[SignatureBytes],
        alg_id: &str,
        messages: &[Vec<u8>],
        signatures: &[SignatureBytes],
    ) -> Result<Vec<bool>, CryptoError> {
        assert_eq!(pub_keys.len(), messages.len());
        assert_eq!(messages.len(), signatures.len());

        match self.backend {
            BatchBackend::Gpu { .. } if messages.len() >= 16 => self.gpu_batch_verify(pub_keys, alg_id, messages, signatures),
            _ => self.rayon_batch_verify(pub_keys, alg_id, messages, signatures),
        }
    }

    fn gpu_batch_verify(
        &self,
        pub_keys: &[SignatureBytes],
        alg_id: &str,
        messages: &[Vec<u8>],
        signatures: &[SignatureBytes],
    ) -> Result<Vec<bool>, CryptoError> {
        self.ensure_gpu()?;
        // Pack into contiguous device memory, launch kernel, read results.
        // Implementation matches the GPU kernel dispatch pattern.
        match SignatureAlgorithm::from_id_string(alg_id)? {
            SignatureAlgorithm::SPHINCS_SHA2_128S => {
                // Use GPU batch SPHINCS+ verification kernel
                let count = messages.len();
                let mut results = vec![0i32; count];
                // ... pack and launch foretias_gpu_batch_sphincs_verify
                // (full implementation in Phase 3)
                c_result_to_error(FORETIAS_OK)?;
                Ok(results.iter().map(|&r| r == 0).collect())
            }
            _ => self.rayon_batch_verify(pub_keys, alg_id, messages, signatures),
        }
    }

    fn rayon_batch_verify(
        &self,
        pub_keys: &[SignatureBytes],
        alg_id: &str,
        messages: &[Vec<u8>],
        signatures: &[SignatureBytes],
    ) -> Result<Vec<bool>, CryptoError> {
        (0..messages.len()).into_par_iter().map(|i| {
            match SignatureAlgorithm::from_id_string(alg_id)? {
                SignatureAlgorithm::SPHINCS_SHA2_128S => {
                    super::signing_sphincs::sphincs_verify(&pub_keys[i], &messages[i], &signatures[i])
                }
                SignatureAlgorithm::Ed25519 => {
                    let pk: [u8;32] = pub_keys[i][..32].try_into().map_err(|_| CryptoError::BadKey)?;
                    let sig: [u8;64] = signatures[i][..64].try_into().map_err(|_| CryptoError::BadSignature)?;
                    // Use crypto server verify_ed25519 via FFI
                    crate::core::signing::ed25519_verify(
                        &ForetiasPubKey32 { bytes: pk },
                        &messages[i],
                        &ForetiasSig64 { bytes: sig },
                    )
                }
                SignatureAlgorithm::Dilithium3 => {
                    super::signing_dilithium::dilithium3_verify(&pub_keys[i], &messages[i], &signatures[i])
                }
            }
        }).collect()
    }
}

// ── Batch HMAC-SHA256 ──────────────────────────────────────

impl BatchCryptoEngine {
    /// Batch HMAC-SHA256 — GPU or rayon dispatch.
    pub fn batch_hmac_sha256(
        &self,
        keys: &[&[u8]],
        messages: &[&[u8]],
    ) -> Result<Vec<ForetiasHash32>, CryptoError> {
        match self.backend {
            BatchBackend::Gpu { .. } if messages.len() >= 64 => self.gpu_batch_hmac(keys, messages),
            _ => self.rayon_batch_hmac(keys, messages),
        }
    }

    fn gpu_batch_hmac(&self, keys: &[&[u8]], messages: &[&[u8]])
        -> Result<Vec<ForetiasHash32>, CryptoError>
    {
        self.ensure_gpu()?;
        let count = messages.len();
        let mut outputs = vec![unsafe { std::mem::zeroed() }; count];
        // ... call foretias_gpu_batch_hmac_sha256
        c_result_to_error(FORETIAS_OK)?;
        Ok(outputs)
    }

    fn rayon_batch_hmac(&self, keys: &[&[u8]], messages: &[&[u8]])
        -> Result<Vec<ForetiasHash32>, CryptoError>
    {
        use hmac::{Hmac, Mac};
        type HmacSha256 = Hmac<sha2::Sha256>;

        (0..messages.len())
            .into_par_iter()
            .map(|i| {
                let mut mac = HmacSha256::new_from_slice(keys[i])
                    .map_err(|_| CryptoError::BadKey)?;
                mac.update(messages[i]);
                let result = mac.finalize().into_bytes();
                let mut out = unsafe { std::mem::zeroed() };
                out.bytes.copy_from_slice(&result);
                Ok(out)
            })
            .collect()
    }
}
```

### 4.3 BatchCryptoEngine Integration with CryptoServer

Both `SoftwareCryptoServer` and `GpuCryptoServer` hold a `BatchCryptoEngine` for batch operations:

```rust
// Added to both SoftwareCryptoServer and GpuCryptoServer:
pub batch_engine: Arc<BatchCryptoEngine>,

impl CryptoServer for SoftwareCryptoServer {
    // ... existing methods ...

    /// Returns the batch engine for this server.
    fn batch_engine(&self) -> Arc<BatchCryptoEngine> {
        Arc::clone(&self.batch_engine)
    }
}

impl CryptoServer for GpuCryptoServer {
    // ... existing methods ...

    fn batch_engine(&self) -> Arc<BatchCryptoEngine> {
        Arc::clone(&self.batch_engine)
    }
}
```
```

---

## PART 5 — INTEGRATION POINTS

### 5.1 Foretias::stamp() — No Change

Stamp remains single-item. GPU acceleration applies at the batch level:

```rust
// Existing stamp path unchanged. GPU benefit comes from:
// 1. Batch stamping (multiple items)
// 2. Batch verification (integrity_check)
// 3. PMAC integrity pre-filter
```

### 5.2 Chronomatter::integrity_check() — GPU Batch Path

```rust
impl Chronomatter {
    /// GPU-accelerated integrity check — verifies all tick pairs in parallel.
    /// Falls back to CPU if GPU unavailable.
    pub fn integrity_check_gpu(
        &self,
        calendar: &dyn CalendarLookup,
        start: Option<u64>,
        end: Option<u64>,
    ) -> Result<Vec<bool>, NodeError> {
        let records = calendar.get(start.unwrap_or(0), 10_000)?;
        if records.len() < 2 {
            return Ok(Vec::new());
        }

        let tbid_str = hex::encode(calendar.tbid());
        let filtered: Vec<&TickRecord> = records.iter()
            .filter(|t| t.tick_number >= start.unwrap_or(0) && t.tick_number <= end.unwrap_or(u64::MAX))
            .collect();

        if filtered.len() < 2 {
            return Ok(Vec::new());
        }

        // Build verification batches
        let mut pub_keys = Vec::new();
        let mut blobs = Vec::new();
        let mut sigs = Vec::new();

        for i in 0..filtered.len() - 1 {
            let prev = filtered[i];
            let curr = filtered[i + 1];
            // Build MA blob
            let mut ma_blob = Vec::new();
            ma_blob.extend_from_slice(tbid_str.as_bytes());
            ma_blob.extend_from_slice(&prev.tick_number.to_be_bytes());
            ma_blob.extend_from_slice(&prev.public_key);
            ma_blob.extend_from_slice(&curr.tick_number.to_be_bytes());
            ma_blob.extend_from_slice(&curr.public_key);
            ma_blob.extend_from_slice(&curr.stamps_per_tick.to_be_bytes());
            ma_blob.extend_from_slice(&curr.aa_nonce);

            pub_keys.push(curr.public_key.clone());
            blobs.push(ma_blob);
            sigs.push(curr.forward_foretis.clone());
        }

        // GPU batch verify
        if let Ok(results) = batch_sphincs_verify(&pub_keys, &blobs, &sigs) {
            return Ok(results);
        }

        // Fallback to CPU
        self.integrity_check(calendar, start, end)
    }
}
```

### 5.3 PMAC Integrity Pre-Filter

Before expensive SPHINCS+ verification, use PMAC as a fast integrity check:

```rust
/// Fast integrity pre-filter using PMAC.
/// Computes PMAC over calendar slice, compares with stored MAC.
/// If MAC matches, SPHINCS+ verification is likely to pass (skip if trusted).
/// If MAC mismatches, full SPHINCS+ verification is required.
pub fn calendar_pmac_filter(
    calendar: &dyn CalendarLookup,
    pmac_key: &[u8; 16],
    expected_mac: &[u8; 16],
    tick_start: u64,
    tick_count: usize,
) -> Result<bool, NodeError> {
    let records = calendar.get(tick_start, tick_count)?;

    // Concatenate all tick record bytes
    let mut data = Vec::new();
    for rec in &records {
        data.extend_from_slice(&rec.public_key);
        data.extend_from_slice(&rec.forward_foretis);
        data.extend_from_slice(&rec.backward_foretis);
    }

    // Compute PMAC (GPU accelerated if available)
    let mut ctx = std::ptr::null_mut();
    let rc = unsafe { foretias_pmac2_init(pmac_key.as_ptr(), &mut ctx) };
    c_result_to_error(rc).map_err(|e| NodeError::Crypto(e))?;

    let mut mac = [0u8; 16];
    let rc = unsafe { foretias_pmac2_mac(ctx, data.as_ptr(), data.len(), mac.as_mut_ptr()) };
    c_result_to_error(rc).map_err(|e| NodeError::Crypto(e))?;

    unsafe { foretias_pmac2_destroy(ctx); }

    Ok(mac == expected_mac)
}
```

### 5.4 Factory Function

```rust
// p2p/core-engine/src/crypto_server/mod.rs

/// Creates a GPU-backed crypto server if CUDA is available.
/// Falls back to software if GPU unavailable.
pub fn new_best_available(curve: ForetiasCurve) -> Result<Box<dyn CryptoServer>, CryptoError> {
    #[cfg(feature = "gpu")]
    {
        if unsafe { foretias_gpu_available() } != 0 {
            return Ok(Box::new(gpu::GpuCryptoServer::generate(curve, 0)?));
        }
    }
    new_software(curve)
}
```

---

## PART 6 — IMPLEMENTATION PHASES

### Phase 1: GPU Infrastructure (Foundation)

| Sub-task | Files | Description |
|----------|-------|-------------|
| 1.1 | `build.rs` | Add CUDA detection, nvcc compilation for .cu → .cubin |
| 1.2 | `foretias_core.h` | Add `#ifdef FORETIAS_GPU_AVAILABLE` guards, GPU API declarations |
| 1.3 | `gpu_context.c` / `.h` | GPU context init/cleanup, CUDA runtime wrappers |
| 1.4 | `Cargo.toml` | Add `gpu` feature flag, conditional dependencies |
| 1.5 | Tests | GPU availability detection, context init/cleanup |

**Acceptance:** `foretias_gpu_init()` succeeds on CUDA systems, returns error on non-CUDA systems. All existing tests pass.

### Phase 2: SHA-256 Batch Kernel + PMAC

| Sub-task | Files | Description |
|----------|-------|-------------|
| 2.1 | `cuda/sha256_batch.cu` | SHA-256 batch CUDA kernel |
| 2.2 | `gpu_hash.c` | C11 wrapper for batch SHA-256 GPU |
| 2.3 | `pmac.c` | PMAC2 CPU implementation (AES-128 based) |
| 2.4 | `cuda/pmac.cu` | PMAC2 CUDA kernel |
| 2.5 | `cuda/hmac_batch.cu` | HMAC-SHA256 batch CUDA kernel |
| 2.6 | `hmac_batch.c` | C11 wrapper for batch HMAC GPU |
| 2.7 | `core/hashing.rs` | Rust FFI for batch operations |
| 2.8 | Tests | SHA-256 batch correctness, PMAC2 NIST vectors, HMAC-SHA256 batch |

**Acceptance:** Batch SHA-256 matches libsodium single-item hashes. PMAC2 passes NIST test vectors. Batch HMAC matches Rust `hmac` crate.

### Phase 3: SPHINCS+ GPU Acceleration

| Sub-task | Files | Description |
|----------|-------|-------------|
| 3.1 | `cuda/sphincs_hypertree.cu` | SPHINCS+ hypertree parallel CUDA kernel |
| 3.2 | `gpu_sphincs.c` | C11 wrapper for batch SPHINCS+ verification |
| 3.3 | `gpu.rs` | GpuCryptoServer implementation |
| 3.4 | `mod.rs` | `new_best_available()` GPU detection |
| 3.5 | Tests | SPHINCS+ GPU verify matches CPU, batch verify correctness |

**Acceptance:** GpuCryptoServer passes all CryptoServer trait tests. Batch SPHINCS+ verification produces same results as CPU path.

### Phase 4: Integration & Performance

| Sub-task | Files | Description |
|----------|-------|-------------|
| 4.1 | `chronomatter/mod.rs` | `integrity_check_gpu()` method |
| 4.2 | `gpu_batch.rs` | Rust batch operation wrappers |
| 4.3 | Benchmarks | GPU vs CPU comparison for all operations |
| 4.4 | Fallback paths | Graceful degradation when GPU unavailable |
| 4.5 | Integration tests | End-to-end stamp/verify with GPU backend |

**Acceptance:** GPU backend produces identical results to CPU backend. Performance benchmarks show >10x speedup for batch operations.

---

## PART 7 — PERFORMANCE TARGETS

### 7.1 Expected Throughput Improvements

| Operation | CPU (libsodium/PQClean) | GPU (CUDA) | Speedup |
|-----------|------------------------|------------|---------|
| SHA-256 (single) | ~1μs | ~5μs (overhead) | 0.2x (GPU slower for single) |
| SHA-256 (batch 1024) | ~1ms | ~20μs | **50x** |
| SPHINCS+ verify (single) | ~4ms | ~2ms (overhead) | 2x |
| SPHINCS+ verify (batch 256) | ~1s | ~50ms | **20x** |
| PMAC2 (64KB) | ~500μs | ~50μs | **10x** |
| HMAC-SHA256 (batch 1024) | ~10ms | ~200μs | **50x** |

### 7.2 GPU Overhead Considerations

- **Host→Device memory transfer** dominates for small batches. Threshold: ~64 items minimum before GPU beats CPU.
- **Kernel launch overhead** ~5-10μs per launch. Batch operations amortize this cost.
- **SPHINCS+ GPU kernel** benefits from hypertree parallelism but SHAKE-256 (XOF) is sequential within each chain.
- **Sweet spot:** batch sizes 256-4096 items. Below 64, CPU path is faster.

### 7.3 Auto-Selection Threshold

```rust
// Auto-select GPU path when batch size exceeds threshold
const GPU_SHA256_THRESHOLD: usize = 64;
const GPU_SPHINCS_THRESHOLD: usize = 16;
const GPU_HMAC_THRESHOLD: usize = 64;

fn batch_sha256(data: &[&[u8]]) -> Result<Vec<ForetiasHash32>, CryptoError> {
    if data.len() < GPU_SHA256_THRESHOLD {
        return cpu_batch_sha256(data);  // CPU path
    }
    gpu_batch_sha256(data)  // GPU path
}
```

---

## PART 8 — TESTING STRATEGY

### 8.1 Correctness Tests

| Test | Description |
|------|-------------|
| `sha256_batch_matches_single` | Batch GPU hash equals single CPU hash for each item |
| `sha256_batch_nist_vectors` | All NIST SHA-256 test vectors pass |
| `pmac2_nist_vectors` | PMAC2 passes NIST SP 800-38C test vectors |
| `hmac_batch_matches_single` | Batch GPU HMAC equals single CPU HMAC for each item |
| `sphincs_verify_gpu_matches_cpu` | GPU verification result matches CPU for every item |
| `integrity_check_gpu_matches_cpu` | Full integrity check produces identical results |

### 8.2 Performance Benchmarks

| Benchmark | Description |
|-----------|-------------|
| `bench_sha256_batch_1024` | 1024 SHA-256 hashes, GPU vs CPU |
| `bench_sphincs_verify_batch_256` | 256 SPHINCS+ verifications, GPU vs CPU |
| `bench_pmac2_64kb` | PMAC2 on 64KB data, GPU vs CPU |
| `bench_hmac_batch_1024` | 1024 HMAC-SHA256, GPU vs CPU |
| `bench_integrity_check_1000_ticks` | Full calendar integrity check, 1000 ticks |

### 8.3 Fallback Tests

| Test | Description |
|------|-------------|
| `no_gpu_fallback_to_cpu` | GPU unavailable → CPU path used transparently |
| `small_batch_uses_cpu` | Batch < threshold → CPU path used |
| `gpu_error_fallback_to_cpu` | GPU error → retry with CPU path |

---

## APPENDIX A — AES-128 IMPLEMENTATION FOR PMAC2

PMAC2 requires AES-128 encryption. Two options:

**Option A: Libsodium `crypto_aes128`** (recommended)
```c
#include <sodium/crypto_aes128.h>
static void aes128_ecb(const uint8_t key[16], const uint8_t pt[16], uint8_t ct[16]) {
    crypto_aes128_state state;
    crypto_aes128_encrypt(&state, key, pt, 16, ct);
}
```

**Option B: Standalone AES-128** (if libsodium unavailable)
```c
// Inline AES-128 S-box + key schedule + 10 rounds
// ~200 lines, constant-time implementation preferred
```

### PMAC2 Shift-Left (GF(2^128) multiplication by 2)

```c
static void pmac_shift_left(const uint8_t in[16], uint8_t out[16]) {
    uint64_t hi = ((uint64_t)in[0] << 56) | ((uint64_t)in[1] << 48) |
                  ((uint64_t)in[2] << 40) | ((uint64_t)in[3] << 32) |
                  ((uint64_t)in[4] << 24) | ((uint64_t)in[5] << 16) |
                  ((uint64_t)in[6] <<  8) |  (uint64_t)in[7];
    uint64_t lo = ((uint64_t)in[8] << 56) | ((uint64_t)in[9] << 48) |
                  ((uint64_t)in[10] << 40) | ((uint64_t)in[11] << 32) |
                  ((uint64_t)in[12] << 24) | ((uint64_t)in[13] << 16) |
                  ((uint64_t)in[14] <<  8) |  (uint64_t)in[15];

    lo <<= 1;
    hi <<= 1;
    if (in[15] & 0x80) {
        lo ^= 0x87;  // μ polynomial for PMAC
    }
    hi |= (lo >> 56);
    lo &= 0x00FFFFFFFFFFFFFFULL;

    out[0]  = (hi >> 56) & 0xFF; out[1]  = (hi >> 48) & 0xFF;
    out[2]  = (hi >> 40) & 0xFF; out[3]  = (hi >> 32) & 0xFF;
    out[4]  = (hi >> 24) & 0xFF; out[5]  = (hi >> 16) & 0xFF;
    out[6]  = (hi >>  8) & 0xFF; out[7]  =  hi        & 0xFF;
    out[8]  = (lo >> 56) & 0xFF; out[9]  = (lo >> 48) & 0xFF;
    out[10] = (lo >> 40) & 0xFF; out[11] = (lo >> 32) & 0xFF;
    out[12] = (lo >> 24) & 0xFF; out[13] = (lo >> 16) & 0xFF;
    out[14] = (lo >>  8) & 0xFF; out[15] =  lo        & 0xFF;
}
```

---

## APPENDIX B — RELATIONSHIP TO EXISTING SPECS

| Spec | Relationship |
|------|-------------|
| `FORETIAS_1_MVP_SPEC.md` | C11 core pattern extended — same FFI approach, new GPU backend |
| `FORETIAS_3_PQC_INTEGRATION.md` | SPHINCS+ algorithm unchanged — GPU accelerates existing implementation |
| `CALENDAR_ACTIVE_MIRRORING_PLAN.md` | GPU integrity check accelerates mirror verification |
| `DHT_STRESS_TEST_ANALYSIS_PLAN.md` | GPU batch stamp/verify enables larger stress tests |

(@human — this spec preserves the existing C11 core architecture. GPU is an optional backend selectable via `new_best_available()`. All existing crypto paths (Ed25519, SPHINCS+, Dilithium) remain unchanged. PMAC/HMAC are additive — they don't replace signatures, they serve as fast integrity primitives for batch operations. The CUDA kernels are pre-compiled to .cubin and loaded at runtime via the CUDA driver API from C11.)

# END OF GPU CRYPTO ACCELERATION SPECIFICATION
