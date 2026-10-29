# Formal seven-procedure CBPIR benchmark

## Scope

This is the formal CBPIR benchmark. Earlier Phase-1, Phase-2, P0-C, P0-D, and P3 timings remain validation evidence only. No COIE baseline was run. Setup is preprocessing and is excluded from online latency.

## Frozen configurations

| Profile | N | n | ell | |T| | L | Match use | Fetch use |
|---|---:|---:|---:|---:|---:|---:|---:|
| CBPIR-13 | 8192 | 128 | 64 | 3201 | 4096 | 100% | 100% |
| CBPIR-14 | 16384 | 256 | 64 | 6544 | 8192 | 100% | 100% |
| CBPIR-15 | 32768 | 512 | 64 | 13358 | 16384 | 100% | 100% |

The same ordered database is used for FPS and RFPS at each N. There are no physical dummy windows. Fixed 64-slot windows use the validated legacy-frame prefix/zero-extension adapter.

## Method

- Release build; `OMP_NUM_THREADS=1`; `OMP_DYNAMIC=FALSE`.
- Cells ran serially, in ascending N, then FPS/RFPS and K=1,4,8,16 inside each native process.
- Each cell used 3 untimed warmups and 20 measured runs.
- Context generation and KeyGen completed before Setup and were excluded from all seven procedures.
- Setup began after BGL loading and context/key generation. FPS and RFPS Setup are separate because they encode different query-independent CKKS plaintext views.
- Serialization was outside procedure timers. Communication uses serialized protocol ciphertexts only.
- `t_online_sum` is the six online procedure durations. `t_online_outer` is an independent outer in-memory timer around the same work.

## Source-to-procedure map

- Setup: `prepare_fps` / `prepare_rfps` in `cbpir_formal_benchmark.rs`, calling `Catalog::new`, staged P0-C server preparation, and `FormalFetchContext::prepare_database`.
- MatchQuery: staged P0-C `match_query`.
- MatchResponse: staged P0-C `match_response`.
- Locate: staged P0-C `decrypt_idft_scores` followed by P0-D `locate_many_from_scores`; one timer covers decrypt, inverse DFT, score extraction, thresholding, boundary checks, stable-ID mapping, deduplication, sorting, and First_K.
- FetchQuery: P0-D `FormalFetchContext::fetch_query` over P0-A fixed-window selector/encryption.
- FetchResponse: `FormalFetchContext::fetch_response` ciphertext-plaintext evaluation.
- Recover: `FormalFetchContext::recover`, including decrypt, window extraction, zero-extension, unchanged frame decoding, and payload return.

## Timing fixtures

| Profile | Mode | Pattern | m | tau | Measured candidate/selection ranges |
|---|---|---|---:|---:|---|
| CBPIR-13 | FPS | `f` | 1 | 0.5 | K=1: C=16, k=1; K=4: C=16, k=4; K=8: C=16, k=8; K=16: C=16, k=16 |
| CBPIR-13 | RFPS | `f` | 1 | 0.5 | K=1: C=16, k=1; K=4: C=16, k=4; K=8: C=16, k=8; K=16: C=16, k=16 |
| CBPIR-14 | FPS | `,` | 1 | 0.5 | K=1: C=16, k=1; K=4: C=16, k=4; K=8: C=16, k=8; K=16: C=16, k=16 |
| CBPIR-14 | RFPS | `,` | 1 | 0.5 | K=1: C=11–16, k=1; K=4: C=10–16, k=4; K=8: C=15–16, k=8; K=16: C=14–16, k=14–16 |
| CBPIR-15 | FPS | `:` | 1 | 0.5 | K=1: C=17, k=1; K=4: C=17, k=4; K=8: C=17, k=8; K=16: C=17, k=16 |
| CBPIR-15 | RFPS | `:` | 1 | 0.5 | K=1: C=12–17, k=1; K=4: C=0–17, k=0–4; K=8: C=8–17, k=8; K=16: C=10–17, k=10–16 |

Patterns deliberately differ across ring dimensions. Within one N and mode, the pattern is fixed and only K changes. One-byte RFPS timing queries avoid signed cancellation, but CKKS threshold disagreements can still make candidate count and realized k vary; the table reports measured ranges rather than normalizing them.

## Paper-facing mean results

| Profile | Mode | K | Candidate IDs | Realized k | Setup ms | Match ms | Locate ms | Fetch ms | Online ms | Match MiB | Fetch MiB | Total MiB | FPS mismatch total | RFPS CKKS disagreement total | Fetch exact |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| CBPIR-13 | FPS | 1 | 16 | 1 | 1.950 | 13.670 | 3.847 | 5.562 | 23.079 | 1.003 | 0.627 | 1.630 | 0 | 0 | PASS |
| CBPIR-13 | FPS | 4 | 16 | 4 | 1.950 | 18.731 | 4.736 | 7.769 | 31.236 | 1.003 | 0.627 | 1.630 | 0 | 0 | PASS |
| CBPIR-13 | FPS | 8 | 16 | 8 | 1.950 | 15.124 | 6.264 | 9.567 | 30.956 | 1.003 | 0.627 | 1.630 | 0 | 0 | PASS |
| CBPIR-13 | FPS | 16 | 16 | 16 | 1.950 | 15.986 | 4.443 | 5.953 | 26.382 | 1.003 | 0.627 | 1.630 | 0 | 0 | PASS |
| CBPIR-13 | RFPS | 1 | 16 | 1 | 4.908 | 13.005 | 7.097 | 13.215 | 33.317 | 0.752 | 0.627 | 1.380 | 0 | 0 | PASS |
| CBPIR-13 | RFPS | 4 | 16 | 4 | 4.908 | 7.455 | 5.193 | 8.537 | 21.186 | 0.752 | 0.627 | 1.380 | 0 | 0 | PASS |
| CBPIR-13 | RFPS | 8 | 16 | 8 | 4.908 | 7.571 | 4.696 | 10.723 | 22.990 | 0.752 | 0.627 | 1.380 | 0 | 0 | PASS |
| CBPIR-13 | RFPS | 16 | 16 | 16 | 4.908 | 6.665 | 4.004 | 6.165 | 16.834 | 0.752 | 0.627 | 1.380 | 0 | 0 | PASS |
| CBPIR-14 | FPS | 1 | 16 | 1 | 7.957 | 30.441 | 7.875 | 12.753 | 51.069 | 2.003 | 1.252 | 3.255 | 0 | 0 | PASS |
| CBPIR-14 | FPS | 4 | 16 | 4 | 7.957 | 35.632 | 8.472 | 13.657 | 57.761 | 2.003 | 1.252 | 3.255 | 0 | 0 | PASS |
| CBPIR-14 | FPS | 8 | 16 | 8 | 7.957 | 29.098 | 8.628 | 11.748 | 49.475 | 2.003 | 1.252 | 3.255 | 0 | 0 | PASS |
| CBPIR-14 | FPS | 16 | 16 | 16 | 7.957 | 29.281 | 7.564 | 12.928 | 49.773 | 2.003 | 1.252 | 3.255 | 0 | 0 | PASS |
| CBPIR-14 | RFPS | 1 | 11–16 | 1 | 3.021 | 15.359 | 10.330 | 17.286 | 42.976 | 1.502 | 1.252 | 2.755 | 0 | 20 | PASS |
| CBPIR-14 | RFPS | 4 | 10–16 | 4 | 3.021 | 16.135 | 10.133 | 16.572 | 42.840 | 1.502 | 1.252 | 2.755 | 0 | 20 | PASS |
| CBPIR-14 | RFPS | 8 | 15–16 | 8 | 3.021 | 12.591 | 9.480 | 13.417 | 35.488 | 1.502 | 1.252 | 2.755 | 0 | 14 | PASS |
| CBPIR-14 | RFPS | 16 | 14–16 | 14–16 | 3.021 | 14.364 | 9.070 | 17.785 | 41.219 | 1.502 | 1.252 | 2.755 | 0 | 12 | PASS |
| CBPIR-15 | FPS | 1 | 17 | 1 | 7.025 | 55.275 | 15.160 | 24.073 | 94.508 | 4.003 | 2.502 | 6.505 | 0 | 0 | PASS |
| CBPIR-15 | FPS | 4 | 17 | 4 | 7.025 | 74.660 | 17.985 | 28.959 | 121.605 | 4.003 | 2.502 | 6.505 | 0 | 0 | PASS |
| CBPIR-15 | FPS | 8 | 17 | 8 | 7.025 | 71.659 | 15.371 | 24.954 | 111.983 | 4.003 | 2.502 | 6.505 | 0 | 0 | PASS |
| CBPIR-15 | FPS | 16 | 17 | 16 | 7.025 | 73.866 | 21.756 | 30.776 | 126.398 | 4.003 | 2.502 | 6.505 | 0 | 0 | PASS |
| CBPIR-15 | RFPS | 1 | 12–17 | 1 | 5.774 | 39.896 | 18.376 | 33.430 | 91.702 | 3.002 | 2.502 | 5.505 | 0 | 28 | PASS |
| CBPIR-15 | RFPS | 4 | 0–17 | 0–4 | 5.774 | 32.800 | 18.417 | 33.966 | 85.183 | 3.002 | 2.502 | 5.505 | 0 | 36 | PASS |
| CBPIR-15 | RFPS | 8 | 8–17 | 8 | 5.774 | 62.913 | 21.568 | 34.365 | 118.845 | 3.002 | 2.502 | 5.505 | 0 | 29 | PASS |
| CBPIR-15 | RFPS | 16 | 10–17 | 10–16 | 5.774 | 56.959 | 27.654 | 45.490 | 130.104 | 3.002 | 2.502 | 5.505 | 0 | 23 | PASS |

Columns are shaped for later side-by-side COIE insertion. Setup remains separate. Full count, mean, median, sample standard deviation, min, max, and nearest-rank p95 are in the summary CSV files.

## Preserved RFPS evidence (untimed)

| Profile | Query | Category | Candidates | Cancellation | Threshold-only | CKKS disagreements |
|---|---|---|---:|---:|---:|---:|
| CBPIR-13 | rfps-literal | rfps_literal | 1 | 0 | 0 | 0 |
| CBPIR-13 | rfps-cancellation | rfps_cancellation | 2 | 2 | 0 | 0 |
| CBPIR-13 | rfps-threshold-only | rfps_threshold_only | 2 | 0 | 4 | 2 |
| CBPIR-14 | rfps-literal | rfps_literal | 1 | 0 | 0 | 0 |
| CBPIR-14 | rfps-cancellation | rfps_cancellation | 4 | 5 | 0 | 1 |
| CBPIR-14 | rfps-threshold-only | rfps_threshold_only | 13 | 6 | 10 | 3 |
| CBPIR-15 | rfps-literal | rfps_literal | 1 | 0 | 0 | 0 |
| CBPIR-15 | rfps-cancellation | rfps_cancellation | 11 | 11 | 0 | 0 |
| CBPIR-15 | rfps-threshold-only | rfps_threshold_only | 22 | 11 | 24 | 11 |

RFPS semantic cancellation and threshold-only acceptance are not counted as CKKS numerical disagreement. Probes are correctness evidence, not timing samples.

## RFPS numerical limitation

RFPS-14 and RFPS-15 fresh encryptions under the frozen parameters do not always retain all exact decisions at tau=0.5. The dedicated RFPS-15 four-case diagnostic showed no mutation from reused server state. With fixed R=[3], fresh encryption and fresh server state produced 5–17 candidate IDs and 0–12 CKKS threshold disagreements across 50 repetitions. Therefore all RFPS full-pipeline rows report actual per-run candidate counts and realized k; they are not normalized to the oracle or silently replaced.

## Controlled deterministic Fetch-only k scaling

| Profile | k | Fetch mean ms | Fetch median ms | Fetch p95 ms | Query bytes | Response bytes |
|---|---:|---:|---:|---:|---:|---:|
| CBPIR-13 | 1 | 5.006 | 4.993 | 5.164 | 394197 | 263299 |
| CBPIR-13 | 4 | 6.277 | 5.324 | 11.759 | 394197 | 263299 |
| CBPIR-13 | 8 | 5.536 | 5.535 | 5.782 | 394197 | 263299 |
| CBPIR-13 | 16 | 9.592 | 7.563 | 19.078 | 394197 | 263299 |
| CBPIR-14 | 1 | 16.781 | 12.315 | 28.276 | 787413 | 525443 |
| CBPIR-14 | 4 | 12.621 | 12.032 | 17.706 | 787413 | 525443 |
| CBPIR-14 | 8 | 14.558 | 12.270 | 20.451 | 787413 | 525443 |
| CBPIR-14 | 16 | 10.681 | 10.456 | 11.876 | 787413 | 525443 |
| CBPIR-15 | 1 | 39.998 | 29.258 | 67.735 | 1573845 | 1049731 |
| CBPIR-15 | 4 | 29.671 | 27.808 | 36.653 | 1573845 | 1049731 |
| CBPIR-15 | 8 | 34.230 | 28.308 | 44.580 | 1573845 | 1049731 |
| CBPIR-15 | 16 | 43.274 | 34.866 | 81.501 | 1573845 | 1049731 |

These controlled rows use the first k stable IDs of the same frozen database. They measure only FetchQuery, FetchResponse, and Recover. They provide k={1,4,8,16} scaling where RFPS-15 cannot reliably realize k=K end to end.

## Dominant aggregate stage

- CBPIR-13 FPS K=1: Match (13.670 ms mean).
- CBPIR-13 FPS K=4: Match (18.731 ms mean).
- CBPIR-13 FPS K=8: Match (15.124 ms mean).
- CBPIR-13 FPS K=16: Match (15.986 ms mean).
- CBPIR-13 RFPS K=1: Fetch (13.215 ms mean).
- CBPIR-13 RFPS K=4: Fetch (8.537 ms mean).
- CBPIR-13 RFPS K=8: Fetch (10.723 ms mean).
- CBPIR-13 RFPS K=16: Match (6.665 ms mean).
- CBPIR-14 FPS K=1: Match (30.441 ms mean).
- CBPIR-14 FPS K=4: Match (35.632 ms mean).
- CBPIR-14 FPS K=8: Match (29.098 ms mean).
- CBPIR-14 FPS K=16: Match (29.281 ms mean).
- CBPIR-14 RFPS K=1: Fetch (17.286 ms mean).
- CBPIR-14 RFPS K=4: Fetch (16.572 ms mean).
- CBPIR-14 RFPS K=8: Fetch (13.417 ms mean).
- CBPIR-14 RFPS K=16: Fetch (17.785 ms mean).
- CBPIR-15 FPS K=1: Match (55.275 ms mean).
- CBPIR-15 FPS K=4: Match (74.660 ms mean).
- CBPIR-15 FPS K=8: Match (71.659 ms mean).
- CBPIR-15 FPS K=16: Match (73.866 ms mean).
- CBPIR-15 RFPS K=1: Match (39.896 ms mean).
- CBPIR-15 RFPS K=4: Fetch (33.966 ms mean).
- CBPIR-15 RFPS K=8: Match (62.913 ms mean).
- CBPIR-15 RFPS K=16: Match (56.959 ms mean).

## N scaling

- FPS K=1: 4.09x mean online latency from N=8192 to N=32768.
- FPS K=4: 3.89x mean online latency from N=8192 to N=32768.
- FPS K=8: 3.62x mean online latency from N=8192 to N=32768.
- FPS K=16: 4.79x mean online latency from N=8192 to N=32768.
- RFPS K=1: 2.75x mean online latency from N=8192 to N=32768.
- RFPS K=4: 4.02x mean online latency from N=8192 to N=32768.
- RFPS K=8: 5.17x mean online latency from N=8192 to N=32768.
- RFPS K=16: 7.73x mean online latency from N=8192 to N=32768.

## Interpretation limits

These are in-process, single-machine prototype measurements of frozen configurations. They exclude network transport, context generation, KeyGen, input disk I/O, serialization latency, report generation, and correctness assertions. OpenMP is constrained; externally observed process thread counts are in `00_environment.txt`. No deployed-network, multi-client, universal RFPS-rate, or COIE-comparison claim is made.
