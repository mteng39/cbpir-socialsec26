# Public repository manifest

## Implementation

- `Cargo.toml`, `Cargo.lock`, `build.rs` — root package and workspace.
- `src/` — composed protocol, Locate, and five supported binaries.
- `crates/ckks-search/` — CKKS FPS/RFPS Search.
- `crates/bgv-cpir/` — BGV packed CPIR.
- `crates/multi-fetch/` — bounded multi-record Fetch.
- `openfhe/` — pinned OpenFHE bootstrap.
- `scripts/` — experiment drivers and validators.

Supported binaries: `cbpir`, `benchmark`, `pattern_length`, `scalability`, and
`search_correctness`. Historical diagnostics/studies under `tools/` are not part
of supported root interface.

## Inputs and evidence

- `data/source/loghub/` — pinned BGL fixture and license.
- `data/workloads/` and `data/manifests/` — inputs and hashes.
- `evaluation/configuration/` — evaluated parameter report.
- `evaluation/formal-benchmark/` — Table 2 raw outputs and summaries.
- `evaluation/search-correctness/` — literal-oracle and FPS/RFPS evidence.
- `evaluation/bf-coie-baseline/` — estimator, audit, sensitivity, and limits.
- `evaluation/final-paper/` — aggregate-ratio recomputation.
- `evaluation/pattern-length-v1/` — pattern-length extension.
- `evaluation/scalability-v1/` — 512/1,024-record extension.

## Paper-to-evidence map

| Paper result | Support |
|---|---|
| Configuration table | `evaluation/configuration/configuration_report.md` |
| CBPIR performance | `evaluation/formal-benchmark/results/` |
| Search correctness | `evaluation/search-correctness/results/` |
| BF-COIE estimates | `evaluation/bf-coie-baseline/` |
| Aggregate ratios | `evaluation/final-paper/` |
| Pattern-length extension | `evaluation/pattern-length-v1/` |
| Larger-database extension | `evaluation/scalability-v1/` |

## Excluded

- Manuscript, slides, reviewer files, submission packages.
- Superseded generations retained in Git history.
- Generated build/install/cache/editor state.
- Credentials and personal administrative files.
