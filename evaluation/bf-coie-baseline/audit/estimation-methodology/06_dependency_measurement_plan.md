# Next-phase dependency measurement plan

## Purpose

Measure reusable BFV and SealPIR kernels needed by the symbolic estimator. This next phase still should not implement the complete BF-COIE protocol or populate Table 2.

## Frozen conditions

- Same VPS as CBPIR.
- Microsoft SEAL 3.6.6 at commit `d045f1beff96dff0fccc7fa0c5acb1493a65338c`.
- SealPIR candidate commit `c7916baa05b43a5c96b8de094232a64f168515a2`.
- One process and one thread; record thread observations externally.
- `OMP_NUM_THREADS=1`, `OMP_DYNAMIC=FALSE` where relevant.
- Release build.
- Three untimed warmups and 20 measured runs where practical.
- `N={8192,16384,32768}`, `n={128,256,512}`, and 64-byte records.
- Runtime verification of BFV parameters, standard security, batching, noise budget, and no plaintext wraparound.

## Auxiliary setup measurements

Measure but keep outside post-Match latency:

- BFV context generation;
- BFV key generation, including required Galois keys;
- SealPIR database preprocessing;
- database memory and serialized setup material.

## Required kernels

### Count reduction

Measure an actual reduction of `n` independently encrypted indicator ciphertexts. Do not estimate it as `n` times one isolated addition. Keep input copies and allocation policy identical across runs. Record additions, output correctness, and noise budget.

### BF hierarchy accumulation

For every matrix cell, instantiate the chosen local integer `ell`, seeded hash functions, and all levels. Measure the entire accumulation kernel, including accumulator initialization, collision-dependent additions, ciphertext copies/moves, and memory allocation. Report actual additions, occupied cells, and hashes. This kernel replaces naïve scalar multiplication by `tau_add`.

### Batch decryption

Measure decryption and decoding of `c` BFV ciphertexts as one kernel. Record cold-first and steady-state behavior. Include plaintext extraction but exclude BF tree traversal.

### Decode traversal

On decrypted integer BF arrays, measure seeded hashing plus Algorithm 2 traversal. Retain actual BF.Check count, true positives, false positives, and abort status. Use deterministic seeds, then repeat across a declared seed set to measure collision variability.

### Serialization

Serialize actual encrypted count, every unbatched BF cell, and every SealPIR request/response with the chosen library's canonical serialization. Measure bytes, serialization time, and deserialization time separately. Do not substitute paper averages.

### SealPIR

For each `(N,n,64-byte record)` profile, measure:

- one query generation;
- one server answer;
- one reconstruction;
- a complete batch of `s+16` independent requests for `s={1,4,8,16}`;
- actual query and response bytes.

Measure both per-request and batch kernels. The batch kernel is authoritative because database-cache reuse, allocations, and preprocessing amortization can make simple multiplication inaccurate.

## Feasibility gates before timing

1. Build unchanged SEAL and SealPIR revisions.
2. Validate each BFV context at standard 128-bit security.
3. Confirm whether exact `t=65537` works with the historical SealPIR code. If not, stop and document the required modulus; do not silently change it.
4. Verify 64-byte round trips for all frozen records.
5. Verify Count and BF accumulation do not wrap and decrypt exactly.
6. Record coefficient-modulus primes, total bits, Galois keys, and expansion ratios.
7. Freeze one explicit hierarchy and `ell` rounding rule before measuring.

## Raw evidence shape

Each sample should identify: evidence tag `M`, source commits, machine, build flags, `N`, `n`, `s`, `fp`, `eta`, hierarchy rule, `ell` rule, kernel, run index, duration, object bytes, operation count, correctness, and failure reason.

