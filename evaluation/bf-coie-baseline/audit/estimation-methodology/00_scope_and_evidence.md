# BF-COIE estimation methodology: scope and evidence

## Scope

This directory validates a method for estimating the post-Match BF-COIE work in Choi et al., CCS 2021. It does not implement BF-COIE, run the full protocol, produce the 12 final estimates, or populate the paper.

The intended local matrix is:

| matched ring dimension `N` | database items `n` | sparsity/result capacity `s=K` |
|---:|---:|---|
| 8,192 | 128 | 1, 4, 8, 16 |
| 16,384 | 256 | 1, 4, 8, 16 |
| 32,768 | 512 | 1, 4, 8, 16 |

The boundary begins after Match, with an encrypted indicator vector. BF-COIE Match, full Online time, and a comparable Setup are outside this method.

## Terminology

- **CBPIR Match:** CKKS using OpenFHE/OpenFHE-rs.
- **CBPIR Fetch:** BGV-RNS using OpenFHE/OpenFHE-rs.
- **Matched BF-COIE BFV configuration:** a proposed local Microsoft SEAL 3.6.6 configuration. It is not the authors' recovered parameter tuple.
- **SealPIR:** the historical PIR dependency, kept distinct from CBPIR Fetch.

## Evidence classes

| tag | class | permitted use |
|---|---|---|
| `P` | Class A: paper-derived exact | Structural formula or count only when its integer convention is fixed. |
| `M` | Class B: locally measured dependency primitive/kernel | Future measurements of SEAL/SealPIR kernels and actual serialized objects. None are timing-measured in this phase. |
| `E` | Class C: modeled estimate | Future composition of `P` counts and `M` kernels. Must be labelled estimate. |
| `V` | Class D: paper validation point | External sanity check only. Never fitted or reused as a local constant. |

## Inspected evidence

- Local paper: `literature/3.search-and-retrieval/choi_2021_2109.07708v1.pdf`, especially Section 4.2, Algorithms 1, 2, and 7, Section 7, and Figures 3--5.
- Source audits: `experiments/cbpir-scaling/search-retrieval-baseline-source-audit.md` and `experiments/cbpir-scaling/bf-coie-baseline/00_source_audit.md`.
- Frozen CBPIR configuration: `experiments/cbpir-scaling/phase2-full-capacity-final/`.
- Frozen formal evidence: `experiments/cbpir-scaling/formal-seven-procedure-benchmark/`.
- CBPIR source: `P0-A_bgv_cpir/src/backend.rs` and P0-D `model.rs`, `frame.rs`, `configurable_fixed_fetch.rs`, and `formal_benchmark.rs`.
- Microsoft SEAL tag `v3.6.6`, commit `d045f1beff96dff0fccc7fa0c5acb1493a65338c`.
- Historical SealPIR revision `c7916baa05b43a5c96b8de094232a64f168515a2`.

## Non-results

No number in this directory is a final BF-COIE latency or communication result. Paper values such as 16.7 seconds, 1,323 ciphertexts, about 103 KB per FHE ciphertext, and about 368.6 KB per PIR request remain validation points tagged `V`.

