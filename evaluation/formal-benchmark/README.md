# Formal seven-procedure benchmark

Canonical final CBPIR performance experiment: three ring/database sizes, FPS and RFPS, four public `K` values, and 20 measured repetitions per fixed configuration (480 full-pipeline executions).

## Contents

- `design.md`, `environment.txt`, `benchmark_report.md`, `fixture_manifest.csv` — protocol, machine, and fixture documentation.
- `frozen-inputs/` — immutable Phase-2 configuration and workload evidence.
- `raw/<N>/` — frozen native outputs, process records, and stderr. Matching inputs live in `../../data/workloads/`.
- `results/` — setup, procedure, online, communication, correctness, and Fetch-scaling CSVs.
- `verification/` — frozen test output, command capture, and integrity records.

Paper Table 2's CBPIR values come from `results/05_procedure_raw.csv`, `06_procedure_summary.csv`, `07_online_summary.csv`, and `08_communication.csv` (stored here without their original numeric prefixes where the filenames already retain them).

Current orchestration is under `scripts/benchmark/`. Install the pinned OpenFHE
runtime first; see `REPRODUCE.md`.
