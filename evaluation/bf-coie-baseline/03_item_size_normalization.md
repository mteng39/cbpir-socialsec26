# Normalizing SealPIR from 2-byte to 64-byte items

## Fixed logical representation

The frozen CBPIR controlled Fetch record is exactly 64 bytes:

- 4-byte magic;
- 8-byte stable record ID;
- 2-byte payload length;
- 18--40 payload bytes;
- zero padding through byte 64.

The comparison therefore asks SealPIR to retrieve the same 64-byte logical item. It does not compare Choi et al.'s 2-byte item directly with CBPIR's record.

## Historical SealPIR mechanics

The pinned paper-era source computes:

```text
coefficients_per_element = ceil(8 * item_bytes / logt)
elements_per_plaintext   = floor(N / coefficients_per_element)
database_plaintexts      = ceil(n / elements_per_plaintext)
nvec                     = get_dimensions(database_plaintexts, d)
```

Its recommended example uses `N=4096`, `logt=20`, recursion depth `d=2`, batching enabled, and a minimum dimension of two.

| workload | coefficients/item | items/plaintext | database plaintexts | `nvec` |
|---|---:|---:|---:|---|
| paper `10000 x 2 B` | 1 | 4096 | 3 | `[2,2]` |
| target `128 x 64 B` | 26 | 157 | 1 | `[2,2]` |
| target `256 x 64 B` | 26 | 157 | 2 | `[2,2]` |
| target `512 x 64 B` | 26 | 157 | 4 | `[3,2]` |

Thus a 64-byte record still fits within one BFV plaintext packing group. It does not create 32 independent PIR retrievals. The number of logical PIR requests remains exactly `s+16`.

## Query, response, and server-work effects

For these dimensions, historical SealPIR emits one query ciphertext per recursion dimension and the final response has the same expansion-ratio count. The query/response object shape therefore does not grow by 32x.

For `[a,b]` and expansion ratio `R`, the two recursion levels perform `a*b + b*R` ciphertext-plaintext multiplications. Relative to `[2,2]`, `[3,2]` changes this from `4+2R` to `6+2R`. With the source default `R=12`, the exact ratio is `30/28=15/14`.

The central Fetch estimate therefore uses:

- factor `1` for `n=128` and `n=256`;
- factor `15/14` for `n=512`.

Because Choi et al. did not disclose their full SealPIR/BFV tuple, `15/14` is not unique. It is retained as the best-supported central factor, with a parameter-sensitivity factor from `1` to `1.5`. This conversion is tagged `D+F+X`: source-derived packing, paper-fitted cost, and extrapolation across an unpublished configuration.

## Parameter sensitivity

`P` means explicitly stated by Choi et al.; `S` means determined by SealPIR mechanics; `H` means inferred from the pinned paper-era source/default; `U` means unsupported for the authors' actual run.

| plausible parameterization | evidence | paper `10000 x 2 B` | `128 x 64 B` | `256 x 64 B` | `512 x 64 B` | `n=512` computation factor | confidence |
|---|---|---|---|---|---|---:|---|
| `N=4096, logt=20, d=2, R=12` | `N/logt/d`: H; geometry and `R`: S; actual authors' tuple: U | `[2,2]` | `[2,2]` | `[2,2]` | `[3,2]` | `15/14 = 1.0714` | medium; pinned source recommendation and about 320 KB reply are consistent with the paper's 368.6 KB/request |
| `N=8192, logt=20, d=2` | alternative source-supported SealPIR input; authors' choice: U | `[2,2]` | `[2,2]` | `[2,2]` | `[2,2]` | `1.0` | low; larger ciphertexts are less consistent with reported communication |
| `N=4096, logt=16, d=2, R=18` | source permits it; recommended `logt=20`: H; actual choice: U | `[2,2]` | `[2,2]` | `[2,2]` | `[3,2]` | `42/40 = 1.05` | low |
| `N=4096, logt=12, d=2, R=20` | source permits it; actual choice: U | `[3,2]` | `[2,2]` | `[2,2]` | `[3,3]` | `69/46 = 1.5` | low |
| `N=4096, logt=20, d=3` | source permits it; README predicts about 3.2 MB reply: H/S | `[2,2,2]` | `[2,2,2]` | `[2,2,2]` | `[2,2,2]` | `1.0` | low; communication conflicts with 368.6 KB/request |
| `N=4096, logt=20, d=1` | source permits it; authors' choice: U | `[4]` | `[2]` | `[3]` | `[5]` | `5/4 = 1.25` | low; communication shape differs from the reported request size |

Choi et al. state `P`: 2-byte items, SealPIR, Microsoft SEAL 3.6, BFV, and 368.6 KB per request. They do not state `N`, `logt`, recursion depth, coefficient moduli, or expansion ratio. The pinned source's `N=4096`, `logt=20`, `d=2` recommendation is `H`, not a recovered paper parameter tuple. The 368.6 KB figure supports the two-level family but does not identify it uniquely.

Every listed 64-byte item uses fewer than `N` coefficients, so it remains one logical SealPIR element. No plausible row justifies a 32x request multiplier.

## Communication

The source mechanics preserve the request and final response object counts in these cells, so the estimate retains the paper's approximate 368.6 KB per PIR request. This is not a locally serialized byte count and is tagged `P+D+X`.

## Gate result

The 64-byte Fetch normalization is defensible as a source-derived packing transformation with explicit parameter uncertainty. A naïve `64/2=32` latency or communication multiplier is rejected.
