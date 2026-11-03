# Reproduction and validation

Run from repository root. Frozen evidence is read-only.

## Native dependency

```bash
OPENFHE_BUILD_JOBS=1 ./openfhe/bootstrap_openfhe.sh
export OPENFHE_INSTALL_DIR="$PWD/openfhe/.openfhe/openfhe-1.5.1/install"
export LD_LIBRARY_PATH="$OPENFHE_INSTALL_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export CARGO_BUILD_JOBS=1 OMP_NUM_THREADS=1 OMP_DYNAMIC=FALSE
```

Bootstrap verifies OpenFHE 1.5.1 commit
`1306d14f8c26bb6150d3e6ad54f28dfe1007689e`. Crates pin OpenFHE-rs commit
`3e6a46b1377e89e09414cbca7c0c0140c5338397`.

## Build and smoke test

```bash
cargo test --workspace --locked -- --test-threads=1
cargo build --release --features native --bins --locked
cargo run --release --features native --locked --bin cbpir -- --quick
```

Success requires `bounded_fixture_gate_passed: true` in smoke JSON.

## Evidence checks

Interactive exploratory runs:

```bash
cargo run --release --features native --locked --bin cbpir -- run \
  --num-records 128 --searchable-bytes 16 --payload-bytes 16 \
  --record-slots 64 --capacity 16 --pattern-length 4 \
  --search-mode both --warmups 1 --trials 5
```

Interactive runs generate new workloads and retain both Online statistics.
They do not replace frozen Table 2, R1.4, R2.10, or derived BF-COIE evidence.
Native tests run serially because OpenFHE maintains process-global state.

```bash
PYTHONDONTWRITEBYTECODE=1 python3 scripts/test_public_layout.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/benchmark/test_reports.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/bf-coie/test_estimator.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/search-correctness/test_contract.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/pattern-length/test_contract.py
PYTHONDONTWRITEBYTECODE=1 python3 scripts/scalability/test_contract.py
PYTHONDONTWRITEBYTECODE=1 python3 evaluation/final-paper/test_recompute_final_comparison.py
sha256sum -c SHA256SUMS
```

## Experiment runners

Formal Table 2 benchmark:

```bash
python3 scripts/benchmark/run.py --verify
cargo run --release --features native --locked --bin benchmark \
  < data/workloads/formal_8192_input.json
```

Search correctness:

```bash
python3 scripts/search-correctness/run.py --validate-only
```

Pattern length (`K=16`, `n={128,256,512}`, `m={1,4,8,16}`):

```bash
python3 scripts/pattern-length/run.py
```

Database scale (extended-modulus 512 control and 1,024 records):

```bash
python3 scripts/scalability/run.py
```

Pinned corpus has 1,331 eligible records. Planned 2,048-record profile is
`NOT_RUN`, not extrapolated.

## Interpretation boundaries

- Camera-ready `Online` is sum of independently reported Search, Locate, and
  Fetch medians.
- Communication is serialized message size per configuration.
- CBPIR values are local measurements.
- BF-COIE values are derived estimates, not a local reimplementation.
- Native reruns are hardware-dependent and can need substantial time and memory.
