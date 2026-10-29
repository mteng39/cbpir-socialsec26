# Final remaining BF-COIE methodology audit

## Scope

This audit resolves only latency central models, the `n=512` SealPIR factor, cross-hardware interpretation, and stale estimate artifacts. It does not benchmark BF-COIE, alter communication arithmetic, revisit the 1,323-ciphertext investigation, or edit `cbpir_main.tex`.

## 1. Locate boundary and central model

### Functional boundary

Choi et al. state that evaluation starts when the server holds encrypted match indicators and measures Algorithm 7 step 4 onward. For the functional Locate boundary—encrypted match indicators to candidate indices—the Figure 4 components map as follows:

- `Server`: encrypted Count plus BF-COIE Encode;
- `Client`: decryption of the encrypted count, decryption of the BF hierarchy, and Decode;
- `Retrieval`: SealPIR, therefore Fetch rather than Locate;
- `Communication`: transfer time, excluded from computation latency.

Count is already inside the Figure 4 Server block. The model uses Encode attempts as a calibration proxy for that combined block because Count is not separately measured. Adding another Count term would double-count it.

### Candidate-model audit

Calibration points are Figure 4 at `n=10000` and `s={8,16,32,64,128}`.

Previous central rule was the unweighted average of unrelated floor- and ceil-hierarchy through-origin models:

\[
T_{Locate}^{old}=\frac{1}{2}\left[
(\alpha_{sf}A+\alpha_{cf}c)+(\alpha_{sc}A+\alpha_{cc}c)
\right].
\]

Its sensitivity also included floor/ceil intercept fits after clamping each component at zero. No statistical or protocol argument justified averaging the two hierarchy interpretations.

| model | combined Server+Client RMSE | key issue | target role |
|---|---:|---|---|
| floor hierarchy, through origin | 0.3944 s | higher in-range error than intercept fit, but zero-work boundary and stable downward scaling | central |
| ceil hierarchy, through origin | 0.5545 s | worse residual, represents paper hierarchy ambiguity | sensitivity |
| floor hierarchy, intercept | 0.2624 s | server intercept `-0.9167 s`; clamping distorts small-`n` extrapolation | diagnostic only |
| ceil hierarchy, intercept | 0.2736 s | server intercept `-2.4722 s`; clamping distorts small-`n` extrapolation | diagnostic only |

Floor-origin residuals at increasing `s` are `{-0.2152,-0.2164,-0.4131,-0.3852,+0.6048} s`. Floor is selected because it fits better than ceil and avoids the intercept models' unphysical negative server constants. Averaging floor and ceil has no mathematical justification and is removed.

At the Figure 3/text check point `n=10000,s=16`, floor-origin Locate is `13.3947 s`. Adding anchored Retrieval (`1.9444 s`) and Figure 4 network time (`0.8333 s`) gives `16.1725 s`, versus the paper's `16.7 s` total (`-0.5275 s`). Ceil-origin gives `16.0296 s` (`-0.6704 s`). Intercept fits happen to be closer in range, but their negative server intercepts make them weaker target extrapolators.

Central formula:

\[
T_{Locate}(n,s)=
(7.2222222\times10^{-5})\,2nL_{BF}
+(1.9754069\times10^{-4})\,c
\quad\text{seconds}.
\]

The hierarchy is exact for target cells. Floor/ceil refers only to the non-power-of-two paper calibration workload. All target Locate values are downward extrapolations and have provenance `F+X`.

## 2. Fetch central model

Figure 4 Retrieval is fit against `q=s+16` requests.

Previous `K=1,4` central rule was the arithmetic midpoint

\[
T_{Fetch}^{old}(q)=\frac{T_{origin}(q)+T_{intercept}(q)}{2},
\]

which had no protocol or statistical basis.

| model | formula, seconds | RMSE | role |
|---|---|---:|---|
| intercept | `0.0820685857q - 0.717032557` | 0.0217 s | central for `K=1,4` |
| through origin | `0.0744916953q` | 0.3978 s | sensitivity |

At the lowest observed point, `K=8` and `q=24`, the intercept fit predicts `1.2526 s` versus `1.2500 s` digitized; the origin fit predicts `1.7878 s`. This fit gap is enough to reject a midpoint. The negative intercept is not a claim of negative protocol setup. It is a local empirical extrapolator from `q=24` to `q=20,17`; therefore uncertainty retains the origin fit.

`K=8,16` remain anchored directly to Figure 4 Retrieval at `1.2500 s` and `1.9444 s`. The anchors still require 64-byte/database-geometry extrapolation, so provenance remains `D+F+X`.

## 3. Audit of the `n=512` factor

### What the paper fixes

Choi et al. explicitly state (`P`) Microsoft SEAL 3.6, BFV, SealPIR, 2-byte items, and about 368.6 KB per PIR request. They do not state the SealPIR polynomial degree, usable plaintext bits, recursion depth, coefficient-modulus tuple, or expansion ratio (`U`).

The pinned paper-era SealPIR commit `c7916baa05b43a5c96b8de094232a64f168515a2` recommends (`H`) `N=4096`, `logt=20`, and `d=2`. SealPIR source then determines (`S`) packing geometry and expansion ratio. Its README estimates a two-level response near 320 KB, broadly consistent with—but not uniquely identified by—the paper's 368.6 KB/request. The aggregate Figure 4 Retrieval timings do not expose recursion-level operation counts, so they cannot identify the tuple independently.

### Sensitivity matrix

| plausible parameterization | evidence | paper `10000x2B` | `128x64B` | `256x64B` | `512x64B` | `n=512` factor | confidence |
|---|---|---|---|---|---|---:|---|
| `N4096/logt20/d2/R12` | H+S; actual use U | `[2,2]` | `[2,2]` | `[2,2]` | `[3,2]` | `15/14=1.0714` | medium |
| `N8192/logt20/d2` | plausible source input; actual use U | `[2,2]` | `[2,2]` | `[2,2]` | `[2,2]` | `1.0` | low |
| `N4096/logt16/d2/R18` | source-supported alternative; actual use U | `[2,2]` | `[2,2]` | `[2,2]` | `[3,2]` | `1.05` | low |
| `N4096/logt12/d2/R20` | source-supported alternative; actual use U | `[3,2]` | `[2,2]` | `[2,2]` | `[3,3]` | `1.5` | low |
| `N4096/logt20/d3` | source-supported; communication conflicts | `[2,2,2]` | `[2,2,2]` | `[2,2,2]` | `[2,2,2]` | `1.0` | low |
| `N4096/logt20/d1` | source-supported; communication shape differs | `[4]` | `[2]` | `[3]` | `[5]` | `1.25` | low |

All 64-byte records fit one logical SealPIR element in these configurations. No row supports a 32x item-byte multiplier.

Decision: **RETAIN 15/14 WITH PARAMETER SENSITIVITY**. It is the best-supported paper-era default, but not a recovered authors' parameter. Central factor is `15/14`; range is `1.0--1.5`.

## 4. Hardware comparability

| property | BF-COIE paper | CBPIR formal benchmark |
|---|---|---|
| CPU | Intel Core i9-9900K @ 4.7 GHz | AMD EPYC Processor (family 23/model 1) under KVM; exact clock not exposed |
| visible cores | not reported as benchmark resource | 4 vCPU, one thread/core |
| measured thread mode | single thread | one process at a time; externally observed minimum/maximum one thread; `OMP_NUM_THREADS=1`, `OMP_DYNAMIC=FALSE` |
| RAM | 64 GB | 7.8 GiB, no swap |
| HE library | PySEAL / Microsoft SEAL 3.6 BFV / SealPIR | OpenFHE 1.5.1 via Rust/C++ bridge; CKKS Match and BGV-RNS Fetch |
| implementation runtime | Python wrapper over native SEAL | optimized Rust plus native OpenFHE |
| computation batching | paper states no batching optimization | protocol-native packed encodings; no parallel benchmark execution |

`execution_environment_match = NO`. No frequency, PassMark, Geekbench, or generic IPC normalization is defensible. Workload matching survives: same logical `n`, 64-byte item, and `K`. Computation values may support source-specific absolute reporting, qualitative comparison, and cautious order-of-magnitude discussion. They do not support precise hardware-normalized speedups, exact percentage-faster claims, or attribution of differences solely to protocol design.

## 5. Final 12 latency values

Milliseconds; brackets show sensitivity.

| `n` | `K` | Locate central [low, high] | Fetch central [low, high] | Locate status | Fetch status |
|---:|---:|---:|---:|---|---|
| 128 | 1 | 146.0 [129.6, 146.0] | 678.1 [678.1, 1266.4] | F+X | D+F+X; `K` extrapolated |
| 128 | 4 | 139.9 [123.1, 139.9] | 924.3 [924.3, 1489.8] | F+X | D+F+X; `K` extrapolated |
| 128 | 8 | 154.6 [135.2, 154.6] | 1250.0 [1250.0, 1787.8] | F+X | D+F+X; Figure 4 anchor plus workload extrapolation |
| 128 | 16 | 187.0 [162.5, 187.0] | 1944.4 [1909.2, 2383.7] | F+X | D+F+X; Figure 4 anchor plus workload extrapolation |
| 256 | 1 | 314.8 [280.0, 314.8] | 678.1 [678.1, 1266.4] | F+X | D+F+X; `K` extrapolated |
| 256 | 4 | 278.8 [246.7, 278.8] | 924.3 [924.3, 1489.8] | F+X | D+F+X; `K` extrapolated |
| 256 | 8 | 285.6 [251.4, 285.6] | 1250.0 [1250.0, 1787.8] | F+X | D+F+X; Figure 4 anchor plus workload extrapolation |
| 256 | 16 | 323.3 [282.6, 323.3] | 1944.4 [1909.2, 2383.7] | F+X | D+F+X; Figure 4 anchor plus workload extrapolation |
| 512 | 1 | 686.9 [611.6, 686.9] | 726.6 [678.1, 1899.5] | F+X | D+F+X; `K` and tuple extrapolation |
| 512 | 4 | 584.1 [518.5, 584.1] | 990.4 [924.3, 2234.8] | F+X | D+F+X; `K` and tuple extrapolation |
| 512 | 8 | 564.6 [499.4, 564.6] | 1339.3 [1250.0, 2681.7] | F+X | D+F+X; anchor plus tuple extrapolation |
| 512 | 16 | 589.0 [518.1, 589.0] | 2083.3 [1909.2, 3575.6] | F+X | D+F+X; anchor plus tuple extrapolation |

## 6. Central-value registry

| value | central rule | calibration | status | uncertainty | provenance |
|---|---|---|---|---|---|
| Locate | floor-hierarchy through-origin Server+Client model | Figure 4, `n=10000`, `s=8..128` | downward `n` extrapolation | ceil-hierarchy origin fit | F+X |
| Fetch `K=1` | intercept request fit at `q=17` | Figure 4 Retrieval, `q=24..144` | downward `K` extrapolation | origin request fit | F+X before item normalization; D+F+X final |
| Fetch `K=4` | intercept request fit at `q=20` | same | downward `K` extrapolation | origin request fit | F+X before item normalization; D+F+X final |
| Fetch `K=8` | digitized `1.2500 s` anchor | Figure 4 | direct component anchor; workload extrapolation remains | request fits | P-extracted + D+X |
| Fetch `K=16` | digitized `1.9444 s` anchor | Figure 4 | direct component anchor; workload extrapolation remains | request fits | P-extracted + D+X |
| `n=512` factor | `15/14` | pinned SealPIR recommended tuple | parameter extrapolation | factor `1.0--1.5` | H+S+X |

## 7. Fairness classification

All 12 rows pass logical checks separately: same `n`; same 64-byte logical item; same `K`; `s=K`; `fp=16`; `K+16` requests; no ring equivalence; no direct 2-byte timing comparison; functionally aligned Locate and Fetch boundaries.

- **LOGICAL WORKLOAD COMPARABILITY:** `COMPARABLE WITH ESTIMATION LIMITS` for all 12 cells.
- **COMPUTATION-ENVIRONMENT COMPARABILITY:** `NO` for all 12 cells; values are cross-platform and not hardware-normalized.

## 8. Explicit answers

1. Final Locate model: floor-hierarchy through-origin Figure 4 Server+Client calibration.
2. Final Fetch model for `K=1,4`: request-count intercept fit; origin fit is sensitivity.
3. `K=8,16`: yes, direct Figure 4 Retrieval anchors remain central.
4. `n=512`: `15/14` retained with parameter sensitivity `1.0--1.5`.
5. Twelve Locate values: listed in Section 5 and `08_latency_estimates.csv`.
6. Twelve Fetch values: listed in Section 5 and `08_latency_estimates.csv`.
7. Extrapolations: every Locate value in `n`; Fetch `K=1,4` in `K`; every Fetch value in 64-byte/database geometry; `n=512` additionally in unpublished SealPIR parameters.
8. Logical workloads: yes, comparable with estimation limits.
9. Execution environments: no, unmatched.
10. Hardware limitation: no precise normalized speedup or exact percentage comparison; only cross-platform absolute estimates, qualitative comparison, and cautious order-of-magnitude interpretation.
11. Corrected stale files: `01_paper_data.csv`, `02_paper_extraction.md`, `03_item_size_normalization.md`, `05_model_fits.csv`, `06_model_report.md`, `07_sensitivity.csv`, `08_latency_estimates.csv`, `11_fairness_audit.csv`, `12_table2_proposal.csv`, `13_report.md`, and `14_verification.txt`; estimator and tests were updated. `04`, `09`, and `10` were regenerated byte-identically. `15` was untouched.
12. Values too weak for manuscript use: Setup, Search, full Online, and any precise hardware-normalized speedup remain unavailable. Locate/Fetch central values are usable only as visibly labeled estimates with sensitivity; `n=512` Fetch is weakest because its tuple is not uniquely identified.

## Final decision table

| Issue | Previous treatment | Final treatment | Evidence | Confidence | Effect on estimates |
|---|---|---|---|---|---|
| Locate central | average of floor/ceil origin models | floor-origin only; ceil-origin sensitivity | better floor RMSE; stable zero-work extrapolation; Figure 4 boundary | medium | central Locate changes in all 12 cells; range narrows |
| Locate intercept fits | included in sensitivity after zero clamp | diagnostic only | lower in-range RMSE but negative server intercepts and unstable small-`n` behavior | high | removed from target bounds |
| Fetch `K=1,4` | midpoint of origin and intercept fits | intercept fit central; origin fit sensitivity | RMSE `0.0217` versus `0.3978`; near-exact prediction at `K=8` | medium-high | central Fetch decreases for `K=1,4` |
| Fetch `K=8,16` | Figure 4 anchors | unchanged anchors | direct digitized Retrieval components | high for model choice; figure-extraction uncertainty remains | no central change |
| `n=512` geometry | precise `15/14` central with broad unexplained range | retain `15/14` with explicit `1.0--1.5` parameter sensitivity | pinned defaults and 368.6 KB consistency; authors' tuple absent | medium | central unchanged; basis and uncertainty clarified |
| Hardware fairness | workload fairness only | workload comparable; environment mismatch separate | i9-9900K/64 GB versus virtual AMD EPYC/7.8 GiB; both one thread | high | prohibits precise speedup claims; does not invalidate logical match |
| Stale 1323 ratio | active in old prose/sensitivity | removed from every active rule | completed investigation; formula-derived communication accepted | high | communication central unchanged; unsupported lower bounds removed |
