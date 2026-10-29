# Validation

## Gate result

PASS. Manuscript integration is permitted.

## Checks

- Exactly 480 Search-only executions: 3 profiles x 2 modes x 80 runs.
- Literal IDs independently enumerated from frozen record bytes and matched every native row.
- Every TP/FN/FP value recomputed from serialized record-ID sets.
- Metrics use full deduplicated candidate sets before `First_K`.
- Full candidate generation was identical for K=1,4,8,16 in every run.
- RFPS exact arithmetic reused each run's recorded randomizer and threshold.
- RFPS randomizers matched the 80 retained formal randomizers per profile.
- Boundary-crossing positions were rejected before record candidates.
- Alignment-level causes and record-level errors remain separate.
- No Fetch code or timing/communication aggregation ran.

## Fresh-encryption comparison with retained RFPS evidence

Candidate-count ranges may differ run-by-run because fresh CKKS encryption is known to move threshold-near RFPS scores. The randomizers are identical; the retained and new ranges are:

| N | Retained range | New range |
|---:|---:|---:|
| 8192 | 16--16 | 16--16 |
| 16384 | 10--16 | 15--16 |
| 32768 | 0--17 | 1--17 |

This expected fresh-encryption variability was already established by the retained RFPS diagnostics; it does not change frozen timings or communication values.

## Manuscript integration

- The before/after Table 2 core snapshots contain the same 36 rows and the
  same nine computation/communication cells per row.
- The complete before/after SHA-256 manifests for all three frozen evidence
  directories are byte-identical.
- Table 2 no longer contains the mixed correctness group.
- Table 3 reports the six pre-`First_K` record-level Search-correctness cells.
- `latexmk -pdf` produced `cbpir_main.pdf` with all citations and references
  resolved. No new table-related overfull box was reported.
