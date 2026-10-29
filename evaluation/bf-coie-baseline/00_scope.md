# BF-COIE paper-calibrated estimate scope

## Comparison boundary

This package estimates BF-COIE after Search. It begins with the encrypted match-indicator vector and covers:

- **Locate:** encrypted count, BF-COIE Encode, encrypted Bloom-filter transfer/decryption, and Decode;
- **Fetch:** the protocol-required `s+fp` SealPIR requests.

It does not estimate BF-COIE Setup, Search, or full Online time. Those cells remain `--`. CBPIR values remain measured; BF-COIE values here are paper-derived estimates, not local measurements or a reproduction of the authors' implementation.

## Matched logical workload

The 12 cells use:

- `n in {128,256,512}` records;
- one 64-byte logical retrieval item per record;
- `K=s in {1,4,8,16}`;
- `fp=16`, `eta=2`, and therefore `s+16` PIR requests.

The labels `2^13`, `2^14`, and `2^15` identify the corresponding CBPIR profiles only. No equality between CBPIR's OpenFHE ring dimension and the unpublished BFV parameters of Choi et al. is claimed.

## Evidence tags

| Tag | Meaning |
|---|---|
| `P` | directly reported or graphically extracted from Choi et al. |
| `D` | derived from the published protocol or historical SealPIR packing rules |
| `F` | fitted from published measurements |
| `X` | extrapolated outside a published workload or across an unresolved implementation detail |

Figure-extracted observations are approximate even when tagged `P`. Combined tags retain every dependency, for example `D+F+X`.

## Read-only target context

The estimator was aligned with `experiments/cbpir-scaling/cbpir_main.tex`, especially its Evaluation section, configuration table, performance table, and definitions of Search, Locate, and Fetch. The manuscript is read-only. This package neither modifies it nor fills its placeholders.

## Excluded evidence

No timing from the incomplete local dependency-kernel campaign enters these estimates. No OpenFHE primitive substitutes for BFV or SealPIR. The paper's approximate object sizes are used only where explicitly tagged `P+D+X`.
