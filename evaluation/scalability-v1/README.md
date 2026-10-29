# Larger-database benchmark (R2.10)

This additive benchmark extends the original unsharded CBPIR pipeline beyond
512 records. It keeps one encrypted Fetch query and one encrypted Fetch response.

## Measured profiles

| Records | Searchable bytes | CKKS/BGV ring | FPS Online (ms) | RFPS Online (ms) | FPS / RFPS communication (bytes) |
|---:|---:|---:|---:|---:|---:|
| 512 | 13,358 | 32,768 | 91.073 | 70.339 | 6,821,027 / 5,771,911 |
| 1,024 | 26,524 | 65,536 | 183.025 | 152.580 | 13,636,771 / 11,539,079 |

Online is the sum of independently reported Search, Locate, and Fetch medians.
Each configuration uses 3 warmups and 20 measured executions at `K=16`,
`ell=64`, `m=1`, and plaintext modulus `t=786433`. The 512-record run is a
control using the extended modulus. The pinned BGL workload supplies only 1,331
eligible records, so the planned 2,048-record profile is recorded as `NOT_RUN`
rather than replaced with synthetic data.

## Evidence

- `raw.csv`: execution-level Setup, Search, Locate, Fetch, communication,
  correctness, and process resource measurements.
- `summary.csv`: mean, median, standard deviation, and p95 by size/mode.
- `parameters.csv`: exact workload and HE parameters, plus the 2,048-record gate.
- `fixtures.json`: corpus audit and fixed query evidence.
- `environment.txt`: software, commands, timing, peak RSS, and thread count.
- `verification.txt`: matrix, one-query/one-response, and correctness gates.
- `logs/`: exact inputs, native outputs, stderr, process observations, and the
  retained 2,048-record limitation.

All 80 measured executions succeeded and reconstructed every selected payload
exactly. Reproduction tooling is in `scripts/scalability/`.
