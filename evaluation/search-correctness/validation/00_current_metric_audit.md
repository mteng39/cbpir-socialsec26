# Current Search-Correctness Metric Audit

## Gate result

The retained formal outputs do not contain the per-run CKKS candidate record-ID
sets needed for a literal-ground-truth confusion matrix. A dedicated
correctness-only rerun is therefore required.

## Existing metric

The formal runner builds an exact oracle and then compares two record-ID lists:

1. `selected_ids`: `locate.selected_candidates`, after sorting, deduplication,
   and `First_K`;
2. `exact_first_k`: `oracle.exact_ids.iter().take(public_k)`.

The Boolean `selected_ids_match_exact_first_k` is true exactly when those two
post-`First_K` lists are equal. The existing paper-facing `x/20` values count
this Boolean across 20 measured executions of each fixed `(n, mode, K)` cell.
They do not measure pre-`First_K` recall or precision.

The comparison is record-level, not alignment-level: both lists contain stable
`RecordId` values. The oracle creates at most one `exact_id` per record. Locate
also deduplicates record IDs before selecting its first `K` candidates.

## Meaning by mode

- FPS: the exact score is a sum of squared byte differences. With the retained
  threshold `tau=0.5`, an integer exact score is accepted only when it is zero,
  so `oracle.exact_ids` equals literal-match record IDs.
- RFPS: the exact score is the signed random weighted sum used by RFPS. With
  the same randomizer and threshold as the CKKS execution,
  `oracle.exact_ids` is the exact-arithmetic RFPS candidate set. It is not the
  literal pattern-match set.

Thus exact RFPS semantics were treated as the old RFPS ground truth. Signed
cancellation and threshold-only acceptance were diagnostics, while CKKS
threshold disagreements measured approximate-versus-exact RFPS decisions.

## Why retained raw data are insufficient

`09_correctness.csv` and `native/formal_*_output.json` retain candidate counts,
realized `k`, aggregate disagreement/cause counts, and the post-`First_K`
equality Boolean. They do not retain:

- the full literal record-ID set;
- the full exact-RFPS record-ID set;
- the full pre-`First_K` CKKS candidate record-ID set.

Counts cannot recover set intersections. For example, equal ground-truth and
candidate counts do not distinguish `(TP=16,FN=0,FP=0)` from
`(TP=15,FN=1,FP=1)`. No rigorous TP/FN/FP reconstruction is possible from the
retained aggregates alone.

## New metric boundary

The new evidence will compare the full `locate.candidates` record-ID set with
independently enumerated literal record IDs. This occurs after CKKS recovery,
thresholding, boundary filtering, stable-ID mapping, and deduplication, but
before `selected_candidates` and `First_K`. Exact RFPS remains an attribution
layer only.
