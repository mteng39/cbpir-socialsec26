# Prototype P0-D — bounded BGL K=1 evidence

**K=1 realistic-workload checkpoint.**

Date: 2026-09-11 UTC.

This directory is the immutable realistic evidence checkpoint for the existing
K=1 P0-D composition. It uses BGL-derived record payloads, not complete BGL
lines. It is separate from the synthetic stress study; rates are never pooled.

## Frozen input

Source: official Loghub `BGL/BGL_2k.log`, commit
`dd61d0952749ee7963bde24220d1be5ede023033`, 2,000 logical lines, SHA-256
`2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496`.

The record is the unchanged byte suffix after the first nine whitespace
boundaries. Complete source lines are 124–504 bytes and exceed P0-D's 114-byte
P0-A payload bound. Therefore this evidence establishes exact reconstruction of
the selected message suffix, not the complete source line.

The score-blind hash rule selected 24 stable source-line `RecordId` values:

~~~text
146, 160, 167, 189, 212, 218, 317, 401, 409, 486, 560, 567,
704, 871, 883, 910, 915, 1037, 1117, 1180, 1565, 1622, 1660, 1703
~~~

The payload total is 595 bytes. With 23 zero guards, the P0-C search text is
618 bytes. The input files were generated and hashed before native HE output
was inspected:

- `catalog.json`: 24 records and provenance;
- `patterns.json`: 13 score-blind patterns and literal-selection oracle;
- `cases.jsonl`: 643 complete requests plus independent expected results;
- `study_config.json`: fixed denominators and seed.

Input SHA-256 values appear in `SHA256SUMS`. The catalog and pattern rules are
defined in the approved design note:
`docs/project/p0d/2026-09-11-p0d-k1-bgl-study-design.md`.

## Matrix

- 13 FPS controls at threshold 0.5;
- 13 patterns × 16 seeded uniform-3–32 weight draws × thresholds 0.5, 1, and 4
  = 624 RFPS attempts;
- two controlled patterns × all-one weights × three thresholds = 6 diagnostic
  RFPS attempts;
- total: 643 requests.

Ten patterns are natural corpus substrings: three unique, three occurring in
multiple records, three absent from the selected catalog but taken from
unselected BGL rows, and one repeated twice inside one record. One pattern is a
guard-crossing layout diagnostic. Two are controlled one-change and balanced
change diagnostics rooted in an unchanged BGL substring.

## Main results

All 643 attempts completed. Every request made one P0-A Fetch. There were 381
real selections and 262 dummy selections. All 381 selected real message-suffix
payloads were reconstructed exactly; every dummy Fetch decoded as empty.

Natural uniform-weight RFPS results use the unit “nonliteral valid
window-decision.” Repeating a window under a different weight draw or threshold
creates a new decision. Thresholds remain separate:

| Threshold | Requests | Nonliteral valid window-decisions | Intrinsic cancellation | Threshold-only acceptance | CKKS false accept | CKKS false reject | Literal/K1 selection agreement |
|---:|---:|---:|---:|---:|---:|---:|---:|
| 0.5 | 160 | 40,784 | 2 | 0 | 0 | 0 | 158/160 |
| 1 | 160 | 40,784 | 2 | 3 | 0 | 1 | 158/160 |
| 4 | 160 | 40,784 | 2 | 40 | 0 | 8 | 141/160 |

Thus both synthetic mechanisms also occur in this bounded realistic-text
study. The table is not a corpus-wide or deployment probability. Higher
thresholds change exact RFPS semantics; they are not merely CKKS tolerances.

Across the full 643-request matrix, CKKS had 13 valid-window false rejections
and zero valid-window false acceptances relative to exact FPS/RFPS decisions.
No literal match was rejected. Fresh CKKS randomness means near-threshold counts
can differ on a repeat.

The 10 natural patterns include three patterns matching two distinct records
and one pattern with two offsets inside one record. That last pattern contributes
one extra within-record offset. This was intentionally required by the selection
rule, so it is a behavior test, not a prevalence estimate.

Locate removed 53 repeated accepted offsets in 53 requests across the full
matrix; 51 occurred in the 480 natural uniform-RFPS requests. The full matrix
examined 197,545 boundary-window decisions. Exact scoring accepted 317; CKKS
accepted 301, with 16 boundary false rejections and no boundary false
acceptances. Locate rejected all 301 CKKS-accepted boundary positions before
Fetch.

Exact-score selection agreement was 634/643. Literal-semantic selection
agreement was 601/643. These are different questions: exact P0-A retrieval of a
selected payload does not make an RFPS-selected record semantically correct.

## Runtime

Native pipeline wall-clock distributions, 643 requests:

| Stage | Median | p95 |
|---|---:|---:|
| Match | 116.330 ms | 389.411 ms |
| Locate | 0.002964 ms | 0.004257 ms |
| Fetch | 5.712 ms | 18.083 ms |
| End-to-end request | 123.400 ms | 416.120 ms |

The end-to-end request timer is measured independently; component medians are
not added. P0-C's Match timing includes setup on each call. Its client timing
also includes exact reference/error/decision instrumentation, so pure
decrypt-plus-IDFT time remains unavailable through the inherited API.

Catalog/layout plus BGV preparation took 501.843 ms in this run. The native
runner took 123.887 s and had 34,424 KiB Linux child peak RSS. These are local
container observations, not network or deployment benchmarks.

## Message sizes and separate HE state

The P0-A BGV probe reported: context 619 bytes, public key 394,153, private key
197,418, query 394,197, and response 263,299. It uses no evaluation key. The
private key is local metadata, not a transmitted message.

Separate P0-C serialization roundtrips on the 618-byte catalog reported the
same values at pattern lengths 5, 8, 16, and 32 in this run:

| Mode | Context | Public key | Private key | Evaluation key | Setup ciphertexts | Query ciphertexts | Response |
|---|---:|---:|---:|---:|---:|---:|---:|
| FPS | 629 | 787,379 | 394,036 | 2,623,379 | 2 × 787,423 | 2 × 787,423 | 525,533 |
| RFPS | 573 | 262,969 | 131,803 | 787,939 | 1 × 263,013 | 2 × 263,013 | 263,213 |

Sizes come from separate randomized serialization executions, not the timed
ciphertexts. No serialized key or ciphertext file is retained. CKKS and BGV
contexts, keys, ciphertexts, parameters, sizes, and security wording remain
separate. The only pipeline bridge is the client's clear selected `RecordId`.

## Human-readable artifacts

The `plots/` directory retains PNG, SVG, and source CSV for:

- candidate funnel;
- exact RFPS semantic false-acceptance heatmap;
- literal occurrence and RecordId multiplicity;
- Match/Locate/Fetch/end-to-end runtime distributions.

All four final PNGs were visually inspected. Percentages in the heatmap include
their numerators and denominators. `attempts.csv`, `summary.csv`,
`multiplicity.csv`, and `summary.json` retain the source tables.

## Reproduce and verify

See `commands.txt` for the complete fresh-directory sequence and
`environment.txt` for the runtime environment. The final checks retained here
report:

- realistic Python: 10/10 passed;
- pre-existing rate-study Python: 13/13 passed;
- Rust pure: 10/10 passed;
- Rust native: 15/15 passed serially;
- main and probe formatting: clean;
- main and probe Clippy: clean with warnings denied;
- probe test/build targets: successful;
- OpenFHE-rs pin: `3e6a46b1377e89e09414cbca7c0c0140c5338397`;
- OpenFHE pin: `1306d14f8c26bb6150d3e6ad54f28dfe1007689e`;
- runtime: 1.5.1 in every native row and size probe.

The realistic analyzer independently rebuilds framed offsets and record spans,
recomputes exact byte scores, validates P0-C decision vectors, reconstructs
Locate boundary/dedup/order/target behavior, and compares Fetch output with the
selected catalog payload. This is an independent plaintext oracle, not an
independent HE implementation or security proof.

Test-first failures and command mistakes are retained in `*_red.txt` and
`retained_command_negatives.txt`. Matplotlib emitted dependency deprecation
warnings during its passing test; the final plot command itself emitted none.

## Boundaries of the evidence

This checkpoint does not establish a universal RFPS false-positive probability,
a production threshold, complete BGL-line retrieval, formal composition
security, malicious security, access-pattern hiding, side-channel resistance,
or deployment performance. Under inherited honest-but-curious and parameter
assumptions, the logical server does not receive plaintext pattern or selected
ID; this local cohosted harness does not prove those assumptions. Pattern
length, layout, K=1, timing, message sizes, and repeated-query linkage remain
visible.

No RFPS correction was attempted. P1, P2, P3, P4, proposal, thesis, and article
files were not modified for this checkpoint.
