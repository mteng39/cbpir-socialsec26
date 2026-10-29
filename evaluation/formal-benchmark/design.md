# Formal Seven-Procedure CBPIR Benchmark Design

## Scope

Measure only the frozen CBPIR configurations from
`phase2-full-capacity-final`. Do not alter their records, ordering, fixed
64-slot Fetch windows, query semantics, or HE profiles. Do not run COIE or
edit paper text.

## Frozen configurations

| Profile | N | n | ell | |T| | L |
|---|---:|---:|---:|---:|---:|
| CBPIR-13 | 8192 | 128 | 64 | 3201 | 4096 |
| CBPIR-14 | 16384 | 256 | 64 | 6544 | 8192 |
| CBPIR-15 | 32768 | 512 | 64 | 13358 | 16384 |

Both Match and Fetch use 100% logical capacity. FPS, RFPS, and BGV retain
their approved profiles under `HEStd_128_classic`.

## Architecture

P0-C gains an additive staged API over the existing server-owned-text ct-pt
implementation. It separates context/key creation, query-independent server
state, encrypted query creation, server evaluation, and client recovery.
Existing public APIs remain callable and retain their behavior.

P0-D gains a thin formal benchmark adapter. It composes actual stage outputs:

```text
Setup
  -> MatchQuery
  -> MatchResponse
  -> Locate (decrypt + IDFT + decisions + record mapping + First_K)
  -> FetchQuery
  -> FetchResponse
  -> Recover
```

P0-A supplies the existing BGV fixed-window primitives directly. P3 remains
untouched. No variable windows or physical dummy windows are introduced.

## Timing boundaries

Context generation and KeyGen occur before Setup and are auxiliary data only.
Setup begins after BGL input is loaded and includes Catalog construction,
search-view DFT/plaintext preparation, frame packing, and BGV plaintext
encoding. FPS and RFPS Setup are measured separately because FPS retains both
text and prefix-square spectra, while RFPS retains only the text spectrum.

Locate begins before CKKS decryption and ends after deterministic `First_K`.
Correctness checks and serialization remain outside all procedure timers.

For each run:

```text
t_online_sum = MatchQuery + MatchResponse + Locate
             + FetchQuery + FetchResponse + Recover
```

An outer timer measures the same six calls. Setup is excluded.

## Fixtures

Each profile uses one existing frozen one-byte pattern for both search modes
and all K values:

| Profile | pattern | frozen source query | candidates |
|---|---|---|---:|
| CBPIR-13 | `f` | `fps-k16-exact-16` | 16 |
| CBPIR-14 | `,` | `fps-k16-exact-16` | 16 |
| CBPIR-15 | `:` | `fps-k16-more-17` | 17 |

Patterns differ across ring dimensions. Within one N/mode cell, only public K
changes. RFPS samples fresh reproducible weights from 3..32 inside
MatchQuery. One-byte patterns prevent signed cancellation and keep the timing
candidate sets stable. Separate untimed frozen RFPS probes preserve structural
cancellation, threshold-only acceptance, and CKKS disagreement evidence.

## Repetitions and execution

Run cells serially with fixed order: N ascending, FPS then RFPS, K in
`1,4,8,16`. Use 3 untimed warmups and 20 measured runs. Run one excluded pilot
cell first and report the projected full-matrix duration before the matrix.

All formal commands set:

```text
OMP_NUM_THREADS=1
OMP_DYNAMIC=FALSE
```

An external monitor records maximum process thread count. Release builds only.

## Evidence

Retain raw samples, procedure summaries, per-run aggregate Match/Locate/Fetch/
Online metrics, actual serialized ciphertext sizes, per-run correctness, exact
commands, environment, frozen-tree before/after hashes, and verification logs.
Communication uses MiB = bytes / 1,048,576. Summary p95 uses nearest-rank;
standard deviation uses the sample definition.

No outlier removal is allowed. Failed correctness runs remain failed evidence.
