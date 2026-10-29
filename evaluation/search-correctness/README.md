# Search correctness

Canonical final correctness evidence combines the v2 base run with five independent 80-execution batches per `(n,m,mode)` cell: 9,600 Search-only executions total.

## Contents

- `scripts/` — Python runner/tests plus Rust source supplied with this experiment.
- `raw/` — complete v2 raw JSONL/CSV and per-ring native outputs. Inputs live in `../../data/workloads/`.
- `results/05_search_correctness_summary.csv` and `06_rfps_error_attribution.csv` — base summaries.
- `results/14_batch_summary.csv`, `15_bootstrap_summary.csv`, `16_replication_report.md` — final pooled/batch evidence.
- `validation/` — environment, provenance, frozen hashes, and final validation.

Paper Table 3 reports the `m=8` and `m=16` cells from the final pooled evidence. The full `m in {1,4,8,16}` matrix remains in `16_replication_report.md` and the CSVs.

The Python helper tests can be run as documented in `../../REPRODUCE.md`. Native rerun is blocked by missing Cargo/build metadata, OpenFHE setup, and workload generator dependencies.
