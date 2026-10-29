# Final audit of the correctness columns

## Decision

Keep the two existing headings, **Search** and **Retr.** Use bare empirical
counts for CBPIR and single-letter evidence markers for BF-COIE:

- `x/20`: empirical successes among the 20 measured runs for that cell;
- `T`: a theoretical/compositional guarantee, not an empirical result;
- `A`: correctness inherited from the assumed underlying PIR primitive.

This is shorter and more precise than repeating `[E]`, `byte-exact`,
`Relaxed-set`, or `PIR-correct` in every row. It also avoids presenting a
theorem as an empirical percentage. A footnote must define the different
evidence classes.

The **Search** cell is a compact table label for final selected-record-set
agreement, not only the numerical accuracy of SearchQuery/SearchResponse. For
CBPIR it checks the selected IDs after Locate and `First_K` against the
applicable plaintext oracle. The **Retr.** cell checks payload reconstruction
conditional on the selected or requested IDs.

## Choi et al. evidence audit

The paper does **not** report empirical correctness experiments, correctness
success rates, abort frequencies, or a number of correctness trials for
BF-COIE. Section 7 reports performance and communication experiments. Section
7.1 explicitly measures from the server already holding the encrypted match
bits `[[b_1]],...,[[b_n]]` through recovery of the records, namely Algorithm 7
steps 4 onward. It does not experimentally evaluate Query+Match correctness
[Choi et al., Section 7.1, pp. 10--11].

Algorithm 7, steps 2--3, assumes a correctly formed encrypted query and
homomorphic evaluation of `b_i=q(x_i)`, with `b_i=1` exactly when record `i`
matches [p. 10]. The evaluation does not validate that assumption. Thus,
Query+Match correctness is inherited from the chosen query circuit and FHE
correctness, rather than supported by a BF-COIE correctness trial.

The exact paper guarantees are:

- PIR correctness is defined as reconstruction of the requested database item
  with probability 1 over the PIR randomness [Section 2, p. 3].
- COIE correctness requires every nonzero position to be decoded with
  probability at least `1-negl(lambda)` and the number of false-positive
  positions to be at most `fp` with overwhelming probability [Definition 3.1,
  p. 5].
- Relaxed secure-search correctness requires, except with negligible
  probability, `S subseteq S'` and `|S' \ S| <= fp`, where `S` is the true
  match set [Section 6.1, p. 9].
- Algorithm 7 aborts when the observed false-positive count `e` exceeds `fp`;
  otherwise it adds `fp-e` dummy indices and runs PIR for `s+fp` requests
  [Algorithm 7, steps 7--8, p. 10]. Dummy indices are padding, not false
  positives.
- Theorem 6.2 composes FHE, COIE, and PIR into the relaxed secure-search
  construction [Section 6.3, p. 9]. The evaluated BF-COIE parameters are
  `fp=16` and `lambda=40` [Section 7.1, p. 11].

Therefore BF-COIE **Search** should be `T`: it denotes the relaxed-set
guarantee, not measured Query+Match accuracy. BF-COIE **Retr.** should be `A`:
payload correctness is conditional on the requested candidate indices and is
inherited from the PIR primitive assumed by Theorem 6.2. `--` would correctly
signal the lack of empirical trials but would discard relevant formal evidence;
`T` and `A` preserve that evidence without manufacturing a percentage.

## CBPIR evidence audit

The primary evidence is
`formal-seven-procedure-benchmark/09_correctness.csv`. It contains 480
measured rows: 24 `(N, mode, K)` cells times 20 runs. The table's Search count
is the number of rows with `selected_ids_match_exact_first_k=True`:

- FPS uses the literal-match plaintext oracle and has `20/20` in all 12 cells.
- RFPS uses the same randomizer and threshold in the plaintext RFPS oracle.
  Its lower counts are oracle-set disagreements, not Fetch failures.

Every one of the 480 measured rows has all of the following set to true:

- `fetch_exact_reconstruction`;
- `selected_windows_exact`;
- `unselected_windows_zero`;
- `unused_tail_zero`.

Therefore every CBPIR **Retr.** cell is `20/20`, including RFPS rows whose
Search count is lower. Retrieval remains byte-exact for the IDs actually
selected by Locate. This is also summarized by
`formal-seven-procedure-benchmark/10_verification.txt`: every measured run was
correctness-checked, FPS had zero oracle mismatches, Locate alone performed
`First_K`, and all selected payloads reconstructed exactly. The controlled
Fetch evidence in `11_fetch_k_scaling_raw.csv` independently records exact
selected windows, zero unselected windows, and a zero unused tail for
`k={1,4,8,16}`.

Both CBPIR columns should use numeric empirical notation because both outcomes
were checked in each of the same 20 measured runs. These are counts, not
percentages. RFPS CKKS threshold-disagreement counts should remain outside this
table: they diagnose numerical decisions, while the Search cell reports the
final selected-set agreement against the same-randomizer plaintext RFPS oracle.

## Final cell contents

| `n` | `K` | BF-COIE Search | BF-COIE Retr. | CBPIR-FPS Search | CBPIR-FPS Retr. | CBPIR-RFPS Search | CBPIR-RFPS Retr. |
|---:|---:|:---:|:---:|:---:|:---:|:---:|:---:|
| 128 | 1  | `T` | `A` | `20/20` | `20/20` | `20/20` | `20/20` |
| 128 | 4  | `T` | `A` | `20/20` | `20/20` | `20/20` | `20/20` |
| 128 | 8  | `T` | `A` | `20/20` | `20/20` | `20/20` | `20/20` |
| 128 | 16 | `T` | `A` | `20/20` | `20/20` | `20/20` | `20/20` |
| 256 | 1  | `T` | `A` | `20/20` | `20/20` | `20/20` | `20/20` |
| 256 | 4  | `T` | `A` | `20/20` | `20/20` | `20/20` | `20/20` |
| 256 | 8  | `T` | `A` | `20/20` | `20/20` | `19/20` | `20/20` |
| 256 | 16 | `T` | `A` | `20/20` | `20/20` | `19/20` | `20/20` |
| 512 | 1  | `T` | `A` | `20/20` | `20/20` | `18/20` | `20/20` |
| 512 | 4  | `T` | `A` | `20/20` | `20/20` | `12/20` | `20/20` |
| 512 | 8  | `T` | `A` | `20/20` | `20/20` | `13/20` | `20/20` |
| 512 | 16 | `T` | `A` | `20/20` | `20/20` | `11/20` | `20/20` |

## Recommended table footnote

> Correctness entries `x/20` report empirical successes in the 20 measured
> runs for that cell; `T` denotes a theoretical/compositional guarantee, and
> `A` denotes correctness inherited from the assumed PIR primitive. CBPIR
> Search reports final selected-record-set agreement with the applicable
> plaintext oracle: literal matching for FPS and the same-randomizer,
> same-threshold RFPS oracle for RFPS. For BF-COIE, Search denotes relaxed
> secure-search correctness: except with negligible probability, all true
> matches are included and at most `fp=16` false positives are returned; the
> protocol aborts above this bound. Retr. denotes byte-exact payload recovery
> conditional on the selected/requested indices. Choi et al. report no
> empirical BF-COIE correctness trial count.

## Final recommendation

Use `x/20`, `T`, and `A` exactly as shown. Do not add percentages, `[E]`, long
cell labels, BF-COIE success rates, or RFPS threshold-disagreement counts. No
new experiment is needed to populate these two columns.
