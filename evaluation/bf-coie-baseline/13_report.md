# BF-COIE paper-derived estimate report

## Result

The 12 post-Search cells remain usable as cross-platform, paper-calibrated estimates with explicit limitations. Workloads are matched by database cardinality, 64-byte logical item size, and result capacity. They are not BF-COIE implementation measurements, ring-parameter-matched results, or hardware-normalized comparisons.

## Central latency rules

Locate uses one model, not an average:

\[
T_{Locate}=7.2222222\times10^{-5}(2nL_{BF})
+1.9754069\times10^{-4}c\quad\text{seconds}.
\]

This is the floor-hierarchy, through-origin Figure 4 calibration. The combined server-plus-client RMSE is `0.3944 s`. The ceil-hierarchy through-origin model supplies sensitivity. Intercept models remain diagnostic because their negative server intercepts are unsafe for extrapolation to `n=128..512`.

Figure 4's Server component already contains encrypted Count plus Encode. Client contains decryption plus Decode. Retrieval is SealPIR. Count is not added twice.

Fetch uses the Figure 4 request-count intercept fit for `K=1,4`:

\[
T_{Fetch}=0.0820685857(K+16)-0.717032557\quad\text{seconds}.
\]

Its RMSE is `0.0217 s`, versus `0.3978 s` for the through-origin fit. `K=8,16` remain anchored to Figure 4 Retrieval at `1.2500 s` and `1.9444 s`. The through-origin fit supplies sensitivity.

For `n=512`, `15/14` remains the central SealPIR computation factor under the pinned source's recommended `N=4096, logt=20, d=2` tuple. Because Choi et al. do not publish their actual tuple, the factor has a `1.0--1.5` parameter-sensitivity interval.

## Estimated computation

Values are milliseconds. Brackets are model/parameter sensitivity, not confidence intervals.

| `n` (CBPIR `N`) | `K=s` | Locate `F+X` | Fetch `D+F+X` | Post-Search central |
|---:|---:|---:|---:|---:|
| 128 (`2^13`) | 1 | 146.0 [129.6, 146.0] | 678.1 [678.1, 1266.4] | 824.1 |
| 128 (`2^13`) | 4 | 139.9 [123.1, 139.9] | 924.3 [924.3, 1489.8] | 1064.2 |
| 128 (`2^13`) | 8 | 154.6 [135.2, 154.6] | 1250.0 [1250.0, 1787.8] | 1404.6 |
| 128 (`2^13`) | 16 | 187.0 [162.5, 187.0] | 1944.4 [1909.2, 2383.7] | 2131.5 |
| 256 (`2^14`) | 1 | 314.8 [280.0, 314.8] | 678.1 [678.1, 1266.4] | 992.9 |
| 256 (`2^14`) | 4 | 278.8 [246.7, 278.8] | 924.3 [924.3, 1489.8] | 1203.1 |
| 256 (`2^14`) | 8 | 285.6 [251.4, 285.6] | 1250.0 [1250.0, 1787.8] | 1535.6 |
| 256 (`2^14`) | 16 | 323.3 [282.6, 323.3] | 1944.4 [1909.2, 2383.7] | 2267.8 |
| 512 (`2^15`) | 1 | 686.9 [611.6, 686.9] | 726.6 [678.1, 1899.5] | 1413.5 |
| 512 (`2^15`) | 4 | 584.1 [518.5, 584.1] | 990.4 [924.3, 2234.8] | 1574.4 |
| 512 (`2^15`) | 8 | 564.6 [499.4, 564.6] | 1339.3 [1250.0, 2681.7] | 1903.9 |
| 512 (`2^15`) | 16 | 589.0 [518.1, 589.0] | 2083.3 [1909.2, 3575.6] | 2672.4 |

All Locate values extrapolate below the paper's measured `n>=1000` range. Fetch `K=1,4` also extrapolates below measured `K>=8`. Fetch `K=8,16` starts from digitized paper anchors but still extrapolates across item size/database geometry; all Fetch values remain `D+F+X`.

## Communication

Communication remains formula-derived and unchanged by this latency audit. Locate uses one encrypted Count ciphertext plus `c` BF ciphertexts, with floor/ceiling `ell` sensitivity. Fetch uses `K+16` requests and Choi et al.'s approximate 368.6 KB/request. Figure 5's unresolved 1,323 count is not used as a correction factor.

## Workload and environment fairness

Every cell matches `n`, 64-byte logical item, and `K=s`; retains `fp=16` and `K+16` PIR requests; aligns Locate and Fetch functional boundaries; and claims no BFV/BGV ring equivalence. Logical workload status is `COMPARABLE WITH ESTIMATION LIMITS`.

Execution environments do not match:

| property | BF-COIE paper | CBPIR benchmark |
|---|---|---|
| CPU | Intel Core i9-9900K @ 4.7 GHz | AMD EPYC Processor, KVM virtual host; exact clock not exposed |
| cores visible / mode | paper reports single-thread execution | 4 vCPU visible; benchmark process externally observed at one thread |
| RAM | 64 GB | 7.8 GiB, no swap |
| HE stack | PySEAL; Microsoft SEAL 3.6 BFV; SealPIR | Rust/C++ bridge; OpenFHE 1.5.1; CKKS Match and BGV-RNS Fetch |
| computation batching | paper states none | protocol-native packed HE operations; one process/thread |

No clock-ratio or benchmark-score normalization is applied. Source-specific absolute values, qualitative comparisons, and order-of-magnitude observations remain possible with a cross-platform caveat. Precise hardware-normalized speedups or percentage-faster claims are unsupported.

## Reporting limits

Setup, Search, and full Online remain `--`. Locate and Fetch can be proposed only as estimates. `n (CBPIR N)` remains the clearest group label because `N` identifies only CBPIR. Manuscript wording and table note are intentionally deferred for separate review.
