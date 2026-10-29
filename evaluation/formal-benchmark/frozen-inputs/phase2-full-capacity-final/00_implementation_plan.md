# Final Fixed-Window Configuration Implementation Plan

**Goal:** Parameterize the retained fixed-window Fetch path, audit all allowed widths, and freeze final full-capacity CBPIR workloads.

**Architecture:** Keep the 128-byte frame codec unchanged. Add a P0-D adapter that validates its meaningful prefix, packs it into one uniform width per database, and restores 128 bytes before existing decode. Reuse Phase-2 Match, Locate, source selection, query logic, and evidence conventions.

**Tech Stack:** Rust 1.91.1, OpenFHE-rs/OpenFHE 1.5.1, Python standard library, CSV/Markdown evidence.

---

### Task 1: Restore and verify the fixed baseline

- [x] Confirm all abandoned variable symbols/files are absent.
- [x] Run focused P0-A and P0-D fixed-frame tests serially.
- [x] Record original Phase-2 SHA-256 values.

### Task 2: Support every allowed fixed width

**Files:**
- Modify: `code_branch/prototypes/P0-A_bgv_cpir/src/backend.rs`
- Test: `code_branch/prototypes/P0-A_bgv_cpir/tests/bgv_cpir.rs`

- [x] Add a failing test for a 224-slot window using floor capacity and zero unused tail.
- [x] Remove only the divisibility rejection; retain floor capacity and bounds checks.
- [x] Run the focused test serially.

### Task 3: Add the fixed-width frame adapter

**Files:**
- Create: `code_branch/prototypes/P0-D_ckks_single_record_composition/src/configurable_fixed_fetch.rs`
- Modify: `code_branch/prototypes/P0-D_ckks_single_record_composition/src/lib.rs`
- Test: `code_branch/prototypes/P0-D_ckks_single_record_composition/tests/final_fixed_native.rs`

- [x] Add failing tests for embedded-length validation, all allowed widths, fitting/rejection, first/last windows, empty selection, `k=0..16`, zero tail, and constant ciphertext sizes.
- [x] Implement the adapter using existing `encode_frame`, `decode_frame`, and P0-A fixed-window APIs.
- [x] Run focused native tests serially.

### Task 4: Add deterministic final workload audit

**Files:**
- Create: `experiments/cbpir-scaling/phase2-full-capacity-final/final_workload.py`
- Create: `experiments/cbpir-scaling/phase2-full-capacity-final/test_final_workload.py`

- [x] Add failing tests for source identity, full eligible-pool distributions, real encoded-slot measurements, all 18 `(N,w)` rows, nested prefixes, and deterministic final selection.
- [x] Implement actual-prefix scanning and profile-specific query regeneration.
- [x] Run Python tests.

### Task 5: Add and run the final correctness runner

**Files:**
- Create: `code_branch/prototypes/P0-D_ckks_single_record_composition/src/bin/cbpir_phase2_fixed_final.rs`
- Modify: `code_branch/prototypes/P0-D_ckks_single_record_composition/Cargo.toml`
- Create: `experiments/cbpir-scaling/phase2-full-capacity-final/run_final.py`

- [x] Validate the runner contract through a bounded N=8192 integration run before the full matrix.
- [x] Implement retained Match/Locate plus configurable fixed Fetch.
- [x] Build once against the pinned native runtime.
- [x] Run each final profile serially and validate before report generation.

### Task 6: Generate and verify evidence

- [x] Produce `01_configuration_report.md` through `07_verification.txt`.
- [x] Preserve diagnostic timings only as non-final supporting data inside verification output when naturally emitted.
- [x] Run serial P0-A/P0-C/P0-D regressions and Python tests.
- [x] Recheck original Phase-2 hashes and P3 diff hash.
- [x] Stop without the formal performance benchmark and without committing.
