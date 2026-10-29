# CBPIR

Rust implementation and reproducibility package for **CBPIR: Content-Based
Private Information Retrieval over Structured Databases via Homomorphic
Encryption** (SocialSec 2026).

CBPIR has three online stages:

1. **Search** — CKKS-based FPS or RFPS finds matching alignments.
2. **Locate** — client maps alignments to record IDs, deduplicates, and selects at
   most `K` records.
3. **Fetch** — BGV retrieves selected records with one packed encrypted query and
   one encrypted response.

## Requirements

- Linux on x86-64 with AVX support
- Rust 1.91.1, CMake, C++ compiler, Git, and Python 3
- At least 4 GiB RAM; native release builds can take several minutes

## Quick start

```bash
git clone https://github.com/mteng39/cbpir-socialsec26.git
cd cbpir-socialsec26

OPENFHE_BUILD_JOBS=1 ./openfhe/bootstrap_openfhe.sh
export OPENFHE_INSTALL_DIR="$PWD/openfhe/.openfhe/openfhe-1.5.1/install"
export LD_LIBRARY_PATH="$OPENFHE_INSTALL_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export CARGO_BUILD_JOBS=1 OMP_NUM_THREADS=1 OMP_DYNAMIC=FALSE

cargo build --release --features native --locked
cargo run --release --features native --locked --bin cbpir -- --quick
```

This workflow was tested in the project Linux container with one build job and
one OpenMP thread. First native build is slow; later runs reuse Cargo artifacts.

## Running

Root package exposes one smoke command and four experiment interfaces:

| Binary | Purpose | Input |
|---|---|---|
| `cbpir` | End-to-end correctness smoke test | CLI flags |
| `benchmark` | Formal Table 2 pipeline | JSON on stdin |
| `search_correctness` | FPS/RFPS correctness campaign | JSON on stdin |
| `pattern_length` | Pattern-length campaign | JSON on stdin |
| `scalability` | Larger-database campaign | JSON on stdin |

Smoke options:

```text
cbpir [--quick] [--match-backend ct_ct|server_text_ct_pt]
```

Use `--quick` for onboarding. Omit it for complete retained fixture matrix.

### Interpreting output

Smoke output is JSON. Main fields:

- `bounded_fixture_gate_passed`: every required fixture and correctness gate
  passed.
- `cases[].run.decisions`: Search scores and acceptance decisions.
- `cases[].run.locate`: client-local candidate mapping and bounded selection.
- `cases[].run.output`: recovered Fetch output for that fixture.
- `cases[].run.timings` and `performance`: in-process prototype timings, not
  network latency.
- `serialization`: serialized query/response sizes per execution.
- `cases[].run.exact_selection_agreement`: selected records agree with the
  exact oracle; `bounded_fixture_gate_passed` summarizes all required gates.

## Validate retained evidence

```bash
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_public_layout.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/benchmark/test_reports.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/bf-coie/test_estimator.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/search-correctness/test_contract.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/pattern-length/test_contract.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/scalability/test_contract.py
PYTHONDONTWRITEBYTECODE=1 python3 evaluation/final-paper/test_recompute_final_comparison.py
```

Native experiment drivers:

```bash
python3 scripts/benchmark/run.py --verify
python3 scripts/search-correctness/run.py --validate-only
python3 scripts/pattern-length/run.py
python3 scripts/scalability/run.py
```

See [REPRODUCE.md](REPRODUCE.md) for full commands and boundaries.

## Repository map

| Path | Purpose |
|---|---|
| `src/` | Composed protocol and supported binaries |
| `crates/ckks-search/` | CKKS FPS/RFPS Search |
| `crates/bgv-cpir/` | BGV packed CPIR core |
| `crates/multi-fetch/` | Bounded multi-record Fetch |
| `openfhe/` | Pinned OpenFHE bootstrap |
| `scripts/` | Benchmark and validation drivers |
| `data/` | Pinned workloads and BGL source fixture |
| `evaluation/` | Frozen raw results, summaries, and comparison evidence |
| `tools/` | Historical studies and diagnostics |

## Evidence

- [Formal benchmark](evaluation/formal-benchmark/README.md) — Table 2 CBPIR data.
- [Search correctness](evaluation/search-correctness/README.md) — FPS/RFPS record
  decisions for `m={1,4,8,16}`.
- [Pattern length](evaluation/pattern-length-v1/README.md) — 24 configurations,
  480 measured executions.
- [Database scale](evaluation/scalability-v1/README.md) — 512-record control and
  measured 1,024-record full pipeline.
- [BF-COIE](evaluation/bf-coie-baseline/README.md) — derived estimator,
  provenance, sensitivity, and limits. These are not local measurements.
- [Final comparison](evaluation/final-paper/README.md) — camera-ready aggregate
  recomputation and R1.3 Online convention.

Frozen paper evidence must not be overwritten. New campaigns need new versioned
output directories.

## Citation

```bibtex
@inproceedings{iasenovets2026cbpir,
  title = {CBPIR: Content-Based Private Information Retrieval over Structured Databases via Homomorphic Encryption},
  author = {Iasenovets, Artur and Tang, Fei},
  booktitle = {NSS-SocialSec 2026},
  year = {2026}
}
```

## License

No repository-wide reuse license file has been selected. Loghub input retains its
upstream license under `data/source/loghub/LICENSE`.
