# Final Replication Validation

## Gate result

PASS FOR STATISTICAL REVIEW. Manuscript remains unchanged pending approval.

## Checks

- Exactly 9,600 Search-only executions: 3 profiles x 4 pattern lengths x 2 modes x 5 batches x 80 runs.
- Exactly 400 final runs exist per `(n,m,mode)` cell and 80 per batch/cell.
- Every run uses its plaintext-selected frozen pattern; no RFPS/CKKS result affected selection.
- Literal IDs independently enumerated from frozen bytes matched every native row.
- Every TP/FN/FP value was recomputed from serialized record-ID sets.
- Exact RFPS reused the same recorded randomizer and `tau` as CKKS RFPS.
- Metrics use deduplicated candidate records before `First_K`.
- Candidate generation was invariant for K=1,4,8,16 in every run.
- Batch 1 and batches 2--5 used byte-identical generated input JSON for each profile.
- Every batch/profile ran as a separate native process using the same release-binary path; each run constructed a fresh encrypted query inside that process.
- `batch_id` was added only while normalizing completed native output; it was not part of any cryptographic input or computation.
- No result was discarded as anomalous.
- Per-run, per-batch, and pooled summaries reproduce from raw JSONL/CSV.
- All 15 native processes were externally observed at exactly one thread.
- All 132 frozen-evidence files and their file set match the pre-run manifest.
- All 36 Table 2 protocol rows retain byte-identical computation/communication core cells.
- No Fetch procedure, performance aggregation, manuscript edit, or PDF build occurred.

## Retained versus five-batch `m=1` candidate ranges

| N | Retained formal range | Five-batch range |
|---:|---:|---:|
| 8192 | 16--16 | 16--16 |
| 16384 | 10--16 | 5--16 |
| 32768 | 0--17 | 0--17 |

Range differences are expected evidence of fresh-encryption threshold variability, not a reason to discard runs. Final inference uses all 400 predefined runs per cell and run-level bootstrap uncertainty.
