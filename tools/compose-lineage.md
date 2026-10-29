# Prototype P0-D — CKKS search and BGV record retrieval

## Status

Implemented and checked on the bounded September 10, 2026 experiment. This is
local composition evidence, not a proof of security, universal correctness, or
practical scale. Two RFPS cases intentionally preserve semantic false matches.
See [findings](../../../docs/project/p0d/2026-09-10-p0d-validation-findings.md)
and [raw evidence and commands](../../../docs/project/experiments/p0d/2026-09-10/validation/README.md).
The separate seeded RFPS rate study preserves and measures cancellation and
threshold-only false matches across varied inputs, weights, and thresholds:
[rate findings](../../../docs/project/p0d/2026-09-10-p0d-rfps-rate-findings.md)
and [raw rate evidence](../../../docs/project/experiments/p0d/2026-09-10/rate-study/README.md).
Its fixed balanced and one-change cases are explained by the
[analytical RFPS note](../../../docs/project/p0d/2026-09-11-p0d-rfps-analytical-note.md)
with [exact enumeration evidence](../../../docs/project/experiments/p0d/2026-09-11/rfps-analysis/README.md).
The bounded BGL message-suffix study is a separate realistic evidence class:
[BGL findings](../../../docs/project/p0d/2026-09-11-p0d-k1-bgl-findings.md)
and [raw BGL evidence](../../../docs/project/experiments/p0d/2026-09-11/bgl-k1-study/README.md).
The same frozen BGL inputs also have a separate K=4 composition checkpoint:
[K=4 BGL findings](../../../docs/project/p0d/2026-09-12-p0d-kgt1-bgl-findings.md)
and [raw K=4 BGL evidence](../../../docs/project/experiments/p0d/2026-09-12/kgt1-bgl/README.md).
FPS/RFPS interpretation and threshold meaning are frozen in the
[Match semantics decision](../../../docs/project/p0d/2026-09-12-p0d-match-semantics-decision.md).
RFPS correction is explicitly excluded by the
[scope decision](../../../docs/project/p0d/2026-09-12-p0d-rfps-scope-decision.md).
Standalone canonical P3 K>1 validation is frozen separately in the
[P3 gate note](../../../docs/project/p0d/2026-09-12-p3-standalone-validation.md)
and [raw evidence](../../../docs/project/experiments/p0d/2026-09-12/p3-standalone-validation/README.md).

The frozen K=1 path imports the read-only `P0-C_ckks_dft_search` and
feature-enabled `P0-A_bgv_cpir` crates. A separate K>1 path composes the same
P0-C Match and client Locate stages with validated P3 BGV Fetch for public
`K in {4, 8, 16}`. The K>1 interface has passed its Phase G implementation
gate; its full synthetic and realistic evidence belong to later Phase H and I
checkpoints. P0-D does not depend on P4.

## Interface and code map

```text
Pipeline::prepare(records) -> sorted catalog + separate BGV database/keys
Pipeline::run(request)
  -> P0-C Match, including client decryption and inverse DFT
  -> client Locate(scores, accepted positions, catalog spans, request)
  -> one real or dummy physical target
  -> P0-A encrypted Fetch
  -> Option<ReconstructedRecord> with exact ID and payload bytes

MultiPipeline::prepare(records, K), K in {4, 8, 16}
MultiPipeline::run(request)
  -> P0-C Match, including client decryption and inverse DFT
  -> client Locate, boundary rejection, stable-ID deduplication and ordering
  -> first K stable IDs, with explicit truncation count
  -> stable ID to catalog physical-row mapping
  -> one P3 multi-window BGV Fetch with distinct dummy windows
  -> Vec<ReconstructedRecord> with exact stable IDs and payload bytes
```

- `src/model.rs`, `layout.rs`: types, bounds, sorted ID-to-window mapping.
- `src/match_stage.rs`: calls P0-C `ckks_fps` / `ckks_rfps`. These calls already
  perform client decryption and the inverse discrete Fourier transform (IDFT).
- `src/locate.rs`: validates scores and offsets; rejects boundary crossings,
  deduplicates records, selects one target. Locate performs no HE operations.
- `src/multi_locate.rs`: applies the same boundary and deduplication contract,
  orders stable IDs, and selects at most public K.
- `src/fetch_stage.rs`: calls P0-A `encrypt_query`, `answer`, `decrypt_slots`,
  `decode_fixed_window`; validates the resulting frame.
- `src/multi_fetch_stage.rs`: maps stable IDs to physical rows, calls P3 once,
  maps rows back, validates frames, and checks zero unselected windows.
- `src/pipeline.rs`: orders stages and measures them; it does not implement Locate.
- `src/multi_pipeline.rs`: keeps the K>1 orchestration and timings separate from
  the frozen K=1 API.
- `src/realistic_kgt1.rs`, `src/bin/p0d_bgl_kgt1.rs`: bind the existing frozen
  BGL cases to the K=4 pipeline without changing their selection rules.
- `realistic_kgt1/`: independent K=4 analyzer, mutation tests, CSV summaries,
  and dependency-free SVG plots.
- `src/frame.rs`: exact binary framing, including trailing zero payload bytes.
- `src/oracle.rs`: independent per-record byte comparison and integer scoring.
  It uses neither P0-C reference scores nor Locate's mapping routine.
- `src/report.rs`, `fixtures.rs`, `bin/p0d_experiment.rs`: local experiment runner.
- `probe/`: separate serialization measurement from unchanged P0-C source.

Default features enable pure contracts; `native` enables P0-C, P0-A, and P3.
Native calls must run serially: P0-C manages process-global evaluation keys.
This local harness cohosts client and server roles. It is not a deployed
client/server API or a wire protocol.

## Exact contract

Catalog: 1–63 records; unique nonzero `u64` IDs, sorted numerically. Search fields
are nonempty byte arrays, concatenated with one zero guard between records.
Total text, including guards, has no separate Catalog byte cap. Match computes
`L=next_power_of_two(|T|+1)` and rejects unless `L <= N_CKKS/2` for the selected
P0-C profile. Zero remains valid data.
A match must fit entirely inside one record's half-open span; the guard value
alone never establishes boundary safety.

Pattern length is 1–64 and cannot exceed total text length. Threshold is finite
and nonnegative. Acceptance is inclusive `abs(score) <= threshold`; ordinary
CKKS approximation is not a fuzzy-match specification. Primary threshold: 0.5.
RFPS requires one weight per pattern byte, each in 1–32. Diagnostic weights of 1
are not samples from a uniform 3–32 distribution.

P0-C must return one finite score per text window and strictly ascending, unique,
in-range positions equal to recomputed threshold acceptance. Locate rejects
cross-record windows, keeps the earliest accepted offset per record, orders
candidates by ID, and chooses the smallest ID. Score magnitude does not rank
candidates. Ties and duplicate contents follow the same ID rule. Duplicate IDs
are invalid. Later candidates are counted but not fetched.

Real physical index is the sorted ordinal, 0 through N-1; dummy index is N.
Every successful request makes one Fetch, including an empty result. Dummy
Fetch must decode to zeros and returns `None`. Invalid inputs, component
failures, inconsistent targets, and malformed frames return stage-labelled
errors. An error does not promise that Fetch completed.

For K>1, public K is 4, 8, or 16. Locate retains the earliest position per
stable ID, orders candidates by ascending stable ID, selects the first K, and
reports any rest as truncated. Empty selection is valid. P3 receives physical
row ordinals, not stable IDs, and pads fewer-than-K selections with its distinct
zero dummy windows. P0-D maps every result back through the catalog, validates
the embedded stable ID and payload, and requires every unselected decrypted
window to be zero. A catalog must leave K of the 64 physical windows for P3
dummies.

Real 128-byte frame: `P0D1` + little-endian 8-byte ID + little-endian 2-byte
payload length + 0–114 payload bytes + zero padding. Decode rejects wrong magic,
size, ID, length, or padding. Fetch compares the decoded payload with the original
input as an experiment diagnostic, included in Fetch timing.

## Backend and measurements

- OpenFHE-rs: `3e6a46b1377e89e09414cbca7c0c0140c5338397`.
- OpenFHE: `1306d14f8c26bb6150d3e6ad54f28dfe1007689e`; runtime must report 1.5.1.
- Shared install: `../openfhe_runtime/.openfhe/openfhe-1.5.1/install`.
- BGV: P0-A `prior_mini`, ring 8192, plaintext modulus 65537, 128-byte windows,
  64 physical windows, depth 1, no evaluation keys.
- P3 K>1: the validated one-ring, one-query, one-response multi-window
  extension of the same P0-A BGV backend. P3 row identifiers are physical
  ordinals at this boundary; P0-D stable IDs remain application identifiers.
- CKKS: P0-C's own FPS/RFPS parameters and keys, recorded per run. No CKKS
  ciphertext, context, or key enters BGV. Only the client's clear target connects
  stages. Sharing a native library does not share a security argument.

Outer Match includes P0-C setup on every call. Prepared repetitions reuse BGV
state, not CKKS state. Locate and Fetch have separate wall-clock timings. The
request timer spans Match through Fetch, excluding the independent P0-D oracle.
First call also reports measured preparation-through-Fetch time. Catalog
validation/sorting/layout are measured together. Frame encoding, BGV context,
keys, and database packing are timed separately.

P0-C's `client_decrypt_idft_ns` also times exact reference/error/decision work.
Pure decrypt-plus-IDFT time is unavailable through this API. The derived online
sum inherits that limitation and excludes transport. Message sizes come from
separate randomized serialization roundtrips, not the timed ciphertexts. Context,
keys, setup, queries, and responses are separate; private-key sizes are local
metadata, not network traffic.

## Reproduce

See the raw-evidence README for complete commands, environment, source/linkage
checks, and retained failures. Quick native check from repository root:

```bash
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export PATH="$CARGO_HOME/bin:/usr/local/bin:/usr/bin:/bin"
export OPENFHE_INSTALL_DIR="$PWD/code_branch/prototypes/openfhe_runtime/.openfhe/openfhe-1.5.1/install"
export LD_LIBRARY_PATH="$OPENFHE_INSTALL_DIR/lib"
export CARGO_BUILD_JOBS=1 OMP_NUM_THREADS=1
p0d=code_branch/prototypes/P0-D_ckks_single_record_composition
rtk proxy cargo test --locked --release --manifest-path "$p0d/Cargo.toml" --features native -- --test-threads=1
```

The full experiment uses 11 fixtures and two performance fixtures, each with
one excluded warmup and five repetitions. `--quick` uses one repetition for
schema tests; the full-report verifier rejects it.

The rate study uses a separate `p0d_rates` binary and P0-D-local Python
generator, checker, and plotting scripts under `rate_study/`. It calls the
same unchanged Pipeline for 1,800 native attempts. See its evidence README for
fresh-directory commands and counting rules.

The realistic study uses `p0d_bgl_k1` plus the generator, independent analysis,
and plots under `realistic_study/`. It freezes 24 BGL message-suffix records and
643 attempts before HE execution. Its rates are not pooled with the synthetic
study.

The K=4 realistic study reuses those exact 24 records and 643 cases. It uses
`p0d_bgl_kgt1` and the independent analyzer/plots under `realistic_kgt1/`.
Its counts remain separate from both K=1 realistic and K=4 synthetic evidence.

The CKKS size probe uses a hash-checked, unchanged P0-C source snapshot under
P0-D `target/`. This redirects P0-C's hardcoded serialization output away from
its original source tree. `prepare_probe.sh` refuses to overwrite an existing
snapshot. Verify its saved hashes before reuse; stop if inputs changed.
Generated experimental key files are not retained as evidence.

## Limits and privacy wording

RFPS cancellation or a broad threshold can select a record that does not contain
the pattern. Exact BGV reconstruction does not fix this. No natural CKKS decision
crossing occurred in the retained 11 cases; deterministic injected false positives
and false negatives test Locate's response, not native failure rates. The bounds
are interface limits, not an exhaustively tested domain.

Under inherited honest-but-curious and parameter assumptions, the logical server
interface does not receive the plaintext pattern or selected index. This harness
does not establish those assumptions or prove composition security. The client
sees full scores and candidate IDs. Public layout, pattern length, K=1, message
lengths, timing, phase linkage, and repeated-query behavior remain visible. One
dummy Fetch fixes retrieval count only; it does not prove hidden cardinality.
For K>1, public K and the actual P3 transcript shape are also public; the Phase G
tests do not establish access-pattern hiding or a composition-security proof.
There is no independent security estimate, residual-noise study, network
benchmark, or malicious-party protection.
