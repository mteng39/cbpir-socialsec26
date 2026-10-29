# Correctness-column design audit

## Decision

Retain two correctness columns, but rename them from **Search** and **Retr.** to
**Result set** and **Payload**. The first column must describe the relationship
between the recovered record-ID set and the protocol-defined target set. The
second must describe byte-exact payload recovery conditional on the indices
given to the retrieval procedure.

This is a qualified comparison. CBPIR contributes empirical results from the
20 measured runs in each table cell. BF-COIE contributes a published relaxed
correctness guarantee and the correctness assumption of its underlying PIR.
These evidence types must be marked, not converted into a common percentage.

## 1. Sources and evidence hierarchy

The audit uses the following sources.

1. The current CBPIR manuscript, especially the private-search and
   private-retrieval correctness definitions and Theorem 2 in
   `experiments/cbpir-scaling/cbpir_main.tex`.
2. Formal raw evidence in
   `formal-seven-procedure-benchmark/09_correctness.csv`, the controlled
   Fetch evidence in `11_fetch_k_scaling_raw.csv`, and the acceptance checks in
   `10_verification.txt`.
3. The final configuration/correctness gate in
   `phase2-full-capacity-final/05_correctness.csv` and
   `07_verification.txt`.
4. The RFPS-15 repeated-run diagnostic in
   `formal-seven-procedure-benchmark/rfps15-state-diagnostic/`.
5. Choi et al., especially Definition 3.1 (PDF p. 5), Section 4.2
   (PDF pp. 6--7), Section 6.1 and Theorem 6.2 (PDF p. 9), Algorithm 7
   (PDF p. 10), and the evaluated parameters in Section 7.1 (PDF p. 11).
6. The prior comparison audits `16_remaining_methodology_audit.md`,
   `17_functional_stage_mapping.md`, and
   `18_query_match_estimability_audit.md`.

Phase-1 files are useful development and repair evidence, but they contain
superseded parameter failures. They are not used to populate the proposed
table cells. The formal 20-run evidence is the row-level source of truth.

## 2. Correctness objects for CBPIR

These objects must remain separate.

| Object | Input and output | Correct condition | Role in main table |
|---|---|---|---|
| Search-score numerical correctness | exact scores `M` and recovered CKKS scores `M_tilde` | a reported error norm, principally `||M_tilde-M||_infinity` | diagnostic only; no BF-COIE counterpart |
| Search-decision correctness | exact accepted alignments `I_M(P)` and recovered alignments `I(P)` | `I(P)=I_M(P)` | supporting evidence for Result set |
| Record-selection correctness | accepted alignments through boundary filtering, stable-ID mapping, deduplication, `SortById`, and `First_K` | recovered `S(P)` equals the applicable plaintext-oracle `First_K` ID set | CBPIR meaning of Result set |
| Fetch correctness | selected IDs and recovered BGV windows/records | every selected window and decoded payload is exact; every unselected window and unused tail slot is zero | CBPIR meaning of Payload |
| End-to-end retrieval correctness | pattern and database to returned payloads | returned payloads are exact for the protocol-selected `S(P)` | derived from Result set plus Payload, but not a third table column |

The distinction matters. Exact Fetch with respect to `S(P)` does not prove
that `S(P)` equals a literal-match set. This is especially important for RFPS.

### 2.1 The manuscript theorem

Theorem 2 gives a conditional decision guarantee. If

```text
||M_tilde-M||_infinity <= epsilon
and
min_a ||M_a|-tau| > epsilon,
```

then `I(P)=I_M(P)`. Locate then performs record-boundary filtering,
stable-ID mapping, deduplication, sorting, and `First_K`. Fetch reconstructs
the payloads for the resulting `S(P)` and leaves every other slot zero.

For FPS, `M_a=0` if and only if the substring at `a` equals the pattern. For
RFPS, literal equality implies `M_a=0`, but the converse is false. The theorem
therefore does not make RFPS equivalent to exact string matching.

## 3. CBPIR experimental evidence

### 3.1 Score and decision evidence

The final configuration gate contains explicit plaintext-reference scores,
maximum absolute error, and decision comparisons.

| Mode | Query summaries | Max observed absolute score error | Decision evidence |
|---|---:|---:|---|
| FPS | 42 | 0.0144676803 | 0 decision mismatches; 42/42 exact selections |
| RFPS | 9 | 1.2258316249 | 60 CKKS threshold disagreements; 6/9 approximate selections equal the exact RFPS selection |

The profile-wise maximum absolute errors were
`7.96697e-6`, `0.0144677`, and `7.57577e-5` for FPS-13/14/15, and
`0.247447`, `0.765751`, and `1.22583` for RFPS-13/14/15. These values are
validation diagnostics. They should not become a cross-system correctness
column because Choi et al. report no analogous score-error measurement.

The formal benchmark adds 20 fresh measured executions for every
`(n, mode, K)` cell:

- FPS: 240/240 runs had zero oracle decision mismatches and the exact
  plaintext-oracle `First_K` record-ID set.
- RFPS: 182 alignment-level CKKS threshold disagreements were recorded across
  240 runs. The final `First_K` record-ID set agreed with the same-randomizer
  plaintext RFPS oracle in 212/240 runs.
- Every formal run was correctness-checked. A `success=True` RFPS row does not
  mean oracle agreement: the harness intentionally preserves RFPS numerical
  disagreements while requiring Fetch correctness.

### 3.2 FPS correctness

For the frozen FPS threshold `tau=0.5`, the plaintext FPS oracle is the exact
literal-match oracle. The main-table object is not merely a low score error;
it is equality of the final sorted and truncated stable-ID set with the
plaintext oracle. Each of the 12 FPS cells has 20/20 exact measured runs.

This empirical result is consistent with Theorem 2's threshold-margin
condition. It does not prove correctness for every possible pattern or
database.

### 3.3 RFPS correctness

The correct RFPS oracle uses the same randomizer `R` and threshold `tau` as
the encrypted execution:

```text
M_a(R) = sum_j (T_{a+j}-P_j) R_j
I_M(P;R,tau) = { a : |M_a(R)| <= tau }.
```

Three events must not be merged:

1. **Signed cancellation:** a nonmatching substring has exact score zero.
2. **Threshold-only acceptance:** a nonzero exact score satisfies
   `0 < |M_a| <= tau`.
3. **CKKS threshold disagreement:** exact and recovered scores fall on
   different sides of `tau`.

The first two are part of RFPS semantics. Only the third is an implementation
or numerical disagreement. The retained untimed probes explicitly preserve
literal, cancellation, and threshold-only cases. The RFPS-15 diagnostic also
compares every encrypted run against the exact oracle using that run's `R`.
With fixed `R=[3]`, fresh encryption, and fresh server state, it observed
0--12 threshold disagreements and 5--17 candidate IDs across 50 repetitions;
reused server state was not the cause.

Therefore the main table must use **agreement with the plaintext RFPS oracle**,
not precision against literal matching. Literal-match false acceptance is a
separate semantic property and belongs in RFPS analysis text, not in the
cross-system correctness cell.

### 3.4 Locate and `First_K`

`selected_ids_match_exact_first_k` compares the actual Locate output against
the first `K` stable IDs from the applicable exact plaintext oracle. It covers
boundary rejection, occurrence-to-record mapping, deduplication, ordering,
and truncation. The native regression
`formal_stages_map_stable_ids_and_preserve_fixed_window_fetch` independently
checks 17 candidate records, selection of the first 16 stable IDs, and the
`k=0,1,4,8,16` paths.

An alignment-level disagreement need not change the final `First_K` set. This
explains why RFPS-14 at `K=1` and `K=4` has decision disagreements while all
20 final record-ID sets remain exact.

### 3.5 Fetch correctness

The formal full-pipeline evidence establishes the following for all 480
measured FPS/RFPS runs:

- recovered record IDs and payload bytes equal the selected source records;
- every selected 64-slot window equals the packed source window;
- every unselected window is zero;
- every unused tail slot is zero;
- one encrypted query and one encrypted response are retained.

The controlled Fetch-only evidence adds 240 unique invocations: 60 for each
of `k=1,4,8,16`, covering all three profiles. All 240 were byte-exact, with
selected windows exact, unselected windows zero, and unused tail zero. Its raw
CSV has five procedure rows per invocation; those rows must not be counted as
1,200 independent correctness trials.

The native fixed-window gate additionally sweeps `k=0..17`. The `k=0` response
is all zero, and every nonempty selection reconstructs exactly. Together,
selected-window exactness and unselected-window zero checks rule out observed
cross-window contamination in these tests.

The correct main-table notation is therefore a byte-exact empirical run count,
not merely `PASS` and not a claim over all possible inputs.

## 4. BF-COIE correctness model

Use these symbols to avoid overloading the paper's `S'` before and after dummy
padding:

- `S = {i : q(x_i)=1}`: true matching indices represented by the encrypted
  indicator vector after Algorithm 7, step 3;
- `S_dec`: indices output by BF-COIE Decode before dummy padding;
- `e = |S_dec \ S|`: actual COIE false-positive count;
- `Q_PIR`: the PIR request list after known dummy indices are added.

### 4.1 Match correctness

Algorithm 7, step 3 defines encrypted bits `b_i=q(x_i)`, equal to one for a
match and zero otherwise. Correct homomorphic evaluation is assumed. Choi et
al. do not report an empirical Match-accuracy experiment, and `q` is generic;
it is not an FPS/RFPS string-search oracle. A BF-COIE percentage cannot be
placed beside CBPIR's measured FPS/RFPS decision agreement.

### 4.2 COIE and result-set correctness

Definition 3.1 states two probabilistic properties for a correct
`(n,s,c,fp)`-COIE scheme (Choi et al., PDF p. 5):

```text
Pr[S subseteq S_dec] >= 1-negl(lambda)
Pr[|S_dec \ S| <= fp] >= 1-negl(lambda).
```

Section 4.2 applies these properties to BF-COIE and states that the bottom
Bloom filter, hence the overall construction, has at most `fp` false positives
with all but negligible probability (PDF pp. 6--7). Section 6.1 lifts this to
relaxed secure-search correctness: the client obtains a superset containing
all matches and at most `fp` extra decoded candidates with all but negligible
probability (PDF p. 9). Theorem 6.2 composes COIE and PIR into the complete
secure-search construction.

This is a **theoretical relaxed-set guarantee**, not an empirical percentage
and not exact-set recovery.

### 4.3 Meaning of `fp=16`

Section 7.1 fixes `fp=16` and `lambda=40` (PDF p. 11). The correct
interpretation is:

- `16` is an upper bound on `e` holding with overwhelming probability; it is
  not the number of false positives returned on every run;
- Algorithm 7 aborts before PIR if `e>16`;
- if `e<=16`, the client adds `16-e` known dummy indices;
- therefore `|Q_PIR|=s+16` even when `e<16`;
- dummy indices are not BF-COIE false positives and are not PIR errors.

Algorithm 7 reuses the name `S'` after padding. The comparison table should
instead state the guarantee on `S_dec`, the decoded set before padding. This
keeps COIE false positives distinct from client-chosen dummy requests.

### 4.4 PIR and payload correctness

Choi et al.'s preliminary definition requires PIR reconstruction of the item
at each requested index (PDF p. 3), and Algorithm 7, step 8 runs PIR for the
indices in the padded list (PDF p. 10). Thus, conditional on PIR correctness,
the true, COIE-false-positive, and dummy requests all return the correct record
for their requested index. Correct retrieval of an unwanted candidate or a
known dummy is still PIR-correct.

Choi et al. report no empirical BF-COIE/SealPIR correctness rate. The payload
cell must therefore say `PIR-correct` as an underlying-primitive assumption,
not `100%` and not `20/20`.

### 4.5 End-to-end relaxed correctness

Conditioned on correct Match bits and PIR correctness, a non-aborting run gives
the client every true record and at most 16 additional COIE-candidate records,
except with negligible probability. Known dummy results can be removed because
the client chose those requests. This is not equivalent to exact output-set
precision.

## 5. Apples-to-apples mapping

| Candidate | Assessment | Reason |
|---|---|---|
| A: Search decision / retrieval | **MISLEADING** | CBPIR's decision and stable-ID result are completed in Locate, while BF-COIE's comparable relaxed set is recovered by COIE after Match. BF-COIE Match accuracy is assumed and not empirically reported. A conditional payload column is sound, but the first column is not. |
| B: Search+Locate selected set / BF-COIE recovered set; then retrieval | **FAIR WITH QUALIFICATION** | The objects align functionally, but the heading `Search` would hide Locate and the guarantees differ: CBPIR selects at most `K`; BF-COIE recovers all `s=K` true indices plus at most 16 false positives. |
| C: Result-set correctness / Payload correctness | **FAIR WITH QUALIFICATION** | Both columns compare the same broad objects: recovered indices relative to protocol-defined targets, then correct payloads relative to requested indices. Evidence type and exact-versus-relaxed semantics require footnotes. |
| D: Match decision / Result set / Payload | **MISLEADING for this table** | It is conceptually complete, but BF-COIE Match has no empirical accuracy result and generic query semantics do not match FPS/RFPS. The extra column would be mostly unavailable and would not improve the numerical comparison. |

Candidate C is recommended.

## 6. Numerical values versus guarantees

Percentages should not be used in the main table.

- `100%` hides the number of CBPIR trials and could be mistaken for a proof.
- BF-COIE provides no experimental denominator from which to compute a
  percentage.
- Mixing `100% empirical` with `1-negl(lambda) theoretical` without labels
  would suggest a common statistical quantity when none exists.
- RFPS requires an oracle-qualified count, because exact-string precision is
  not its implementation-correctness metric.

Use compact notation with explicit evidence tags:

- **E** -- empirical validation in the 20 measured runs for that cell;
- **T** -- theoretical guarantee from Choi et al.;
- **A** -- correctness of an underlying primitive assumed by the composed
  construction.

The CBPIR theorem remains supporting theoretical evidence, but the proposed
CBPIR cells report the cell-specific empirical counts. BF-COIE cells do not
invent measured outcomes.

## 7. `K`, `k`, `s`, candidates, and requests

The symbols in the current table have different provenance.

- CBPIR `K` is public capacity. CBPIR `k=|S(P)|` is the observed selected
  count, and may vary for RFPS.
- In the BF-COIE estimate rows, `s=K` is an assumed workload with exactly `K`
  true matches. The displayed `k=K` is therefore an input assumption, not an
  observed decoded count.
- `|S_dec|=s+e`, where `0<=e<=16` on a non-aborting correct run.
- `|Q_PIR|=s+16` after adding `16-e` known dummy indices.

The `k` heading may remain, but the table note must say that BF-COIE's value is
the assumed true-match count. It must not be read as the decoded-candidate
count or PIR-request count.

## 8. Recommended final table design

### 8.1 Exact headings

Keep the group heading `Correctness`. Replace its two subheadings with:

```text
Result set | Payload
```

Do not retain `Search`: the comparable result-set object spans CBPIR Search and
Locate, and lies inside Choi et al.'s native Fetch procedure. Do not retain
`Retr.`: it can be read as end-to-end relevance rather than conditional
payload reconstruction.

### 8.2 Exact compact cell notation

Use these literal forms in the eventual table:

```text
BF-COIE Result set:  S subseteq S_dec; e <= 16 [T]
BF-COIE Payload:     PIR-correct [A]
CBPIR Result set:    x/20 [E]
CBPIR Payload:       20/20 byte-exact [E]
```

Here `x` is the number of measured executions whose final stable-ID set equals
the applicable plaintext oracle. The footnote below supplies the FPS/RFPS
oracle qualification.

### 8.3 Exact row contents

| `n` | `K` | BF-COIE Result set | BF-COIE Payload | CBPIR-FPS Result set | CBPIR-FPS Payload | CBPIR-RFPS Result set | CBPIR-RFPS Payload | RFPS threshold disagreements (supporting evidence; not cell text) |
|---:|---:|---|---|---|---|---|---|---:|
| 128 | 1 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `20/20 [E]` | `20/20 byte-exact [E]` | 0 |
| 128 | 4 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `20/20 [E]` | `20/20 byte-exact [E]` | 0 |
| 128 | 8 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `20/20 [E]` | `20/20 byte-exact [E]` | 0 |
| 128 | 16 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `20/20 [E]` | `20/20 byte-exact [E]` | 0 |
| 256 | 1 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `20/20 [E]` | `20/20 byte-exact [E]` | 20 |
| 256 | 4 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `20/20 [E]` | `20/20 byte-exact [E]` | 20 |
| 256 | 8 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `19/20 [E]` | `20/20 byte-exact [E]` | 14 |
| 256 | 16 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `19/20 [E]` | `20/20 byte-exact [E]` | 12 |
| 512 | 1 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `18/20 [E]` | `20/20 byte-exact [E]` | 28 |
| 512 | 4 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `12/20 [E]` | `20/20 byte-exact [E]` | 36 |
| 512 | 8 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `13/20 [E]` | `20/20 byte-exact [E]` | 29 |
| 512 | 16 | `S subseteq S_dec; e <= 16 [T]` | `PIR-correct [A]` | `20/20 [E]` | `20/20 byte-exact [E]` | `11/20 [E]` | `20/20 byte-exact [E]` | 23 |

The RFPS disagreement column is supporting provenance and is not proposed as
a third correctness column. Its counts are alignment decisions summed over
the 20 runs, not failed run counts.

### 8.4 Exact table footnotes

Recommended correctness note:

> **[E]** Empirical exactness over the 20 measured runs in the corresponding
> cell. For CBPIR, Result set means equality of the final sorted
> `First_K` stable record-ID set with the applicable plaintext oracle: literal
> matching for FPS and the same-randomizer, same-threshold RFPS rule for RFPS.
> RFPS signed cancellation and threshold-only acceptance belong to the RFPS
> semantics; only CKKS threshold crossings are numerical disagreements.
> **[T]** Conditioned on correct Match bits, BF-COIE guarantees, with
> overwhelming probability for `lambda=40`, that every true index is in the
> decoded set `S_dec` and that `e=|S_dec\S|<=16`; it aborts if `e>16`.
> **[A]** BF-COIE payload correctness follows from correctness of the
> underlying PIR; Choi et al. report no empirical correctness rate.

Recommended workload/padding note:

> For BF-COIE, `k=s=K` is the assumed true-match count, not an observed
> decoded-set size. A non-aborting run pads its PIR list to `K+16` requests;
> client-chosen dummy indices are neither COIE false positives nor retrieval
> failures.

### 8.5 Required explanatory manuscript paragraph

> Correctness is separated into result-set recovery and payload
> reconstruction. For CBPIR-FPS, result-set exactness means that the final
> sorted and `First_K`-bounded record-ID set equals the plaintext
> literal-match oracle; for CBPIR-RFPS, it instead means agreement with the
> plaintext RFPS oracle using the same randomizer and threshold. Consequently,
> RFPS cancellation and threshold-only acceptance are protocol semantics,
> whereas CKKS threshold crossings are numerical disagreements. BF-COIE has no
> reported empirical accuracy rate: conditioned on correct Match bits, its
> published guarantee is `S subseteq S_dec` and
> `|S_dec\S|<=16` with overwhelming probability, with abort on a larger
> decoded false-positive count. The Payload column is conditional on the
> selected or requested indices: CBPIR reports byte-exact measured recovery,
> while BF-COIE relies on the correctness of its underlying PIR. The evidence
> tags therefore distinguish empirical validation, theoretical guarantees,
> and underlying-primitive assumptions.

## 9. Additional experiment requirement

No new experiment is needed to populate the recommended cells:

- all CBPIR entries can be extracted from retained 20-run raw evidence;
- BF-COIE Result set is a published theoretical guarantee;
- BF-COIE Payload is explicitly marked as an underlying-PIR correctness
  assumption.

An implemented BF-COIE campaign would be required only to report empirical
false-positive, abort, or PIR failure rates. Those percentages are not needed
and must not be invented. A broad RFPS deployment-rate claim would likewise
require a new query/randomizer study; the proposed cells make only bounded
fixture-specific empirical statements.

## 10. Explicit answers

1. **Should the final table retain two correctness columns?** Yes.
2. **Exact headings?** `Result set` and `Payload`.
3. **Should Search remain?** No. The comparable index-set object spans Search
   and Locate for CBPIR and lies after Match for BF-COIE.
4. **Should Retr. remain?** No. Rename it `Payload` to state the conditional
   object precisely.
5. **What does each measure?** Result set measures recovered record indices
   relative to the protocol-defined target set. Payload measures exact record
   reconstruction conditional on requested indices.
6. **Can BF-COIE receive a value in both?** Yes: a theoretical relaxed-set
   guarantee in Result set and `PIR-correct [A]` in Payload. Neither is an
   empirical percentage.
7. **Can FPS and RFPS use the same metric?** They can use the same set-equality
   metric only when each names its correct oracle. FPS uses literal matching;
   RFPS uses the same-`R`, same-`tau` plaintext RFPS oracle.
8. **Presentation form?** Empirical counts for CBPIR, mathematical guarantee
   notation for BF-COIE Result set, and compact assumption notation for
   BF-COIE Payload. Evidence tags and footnotes are mandatory.
9. **How should `fp=16` appear?** As `e<=16` with overwhelming probability,
   not as exactly 16 false positives and not as a measured rate.
10. **How should no false negatives appear?** As
    `S subseteq S_dec` under tag T, conditioned on correct Match bits and with
    the published overwhelming-probability qualification.
11. **How should RFPS false acceptance appear?** The cell reports equality
    with the plaintext RFPS oracle. Cancellation and threshold-only acceptance
    are named in the footnote as RFPS semantics; CKKS threshold crossings are
    reported separately as numerical disagreements.
12. **How should exact BGV Fetch appear?** `20/20 byte-exact [E]` in every
    CBPIR table row, supported by exact selected windows, zero unselected
    windows, and zero unused tail slots.
13. **Are new experiments necessary?** No, not for the recommended design.
14. **Exact final family cells?** BF-COIE uses
    `S subseteq S_dec; e <= 16 [T]` and `PIR-correct [A]`; every FPS row uses
    `20/20 [E]` and `20/20 byte-exact [E]`; RFPS uses the row-specific counts
    in Section 8.3 and `20/20 byte-exact [E]`.

## 11. Required decision table

| Metric / column | CBPIR-FPS meaning | CBPIR-RFPS meaning | BF-COIE meaning | Same semantic object? | Evidence type | Main-table status |
|---|---|---|---|---|---|---|
| CKKS score error | `||M_tilde-M||` against FPS scores | same norm against same-`R` RFPS scores | no analogous reported score | No | CBPIR empirical | **CBPIR ONLY** |
| Alignment decision agreement | `I(P)=I_M(P)`, where oracle is literal matching at frozen threshold | `I(P)=I_M(P;R,tau)`; cancellation and threshold-only acceptance remain oracle positives | Match bits `b_i=q(x_i)` are assumed correct; no empirical accuracy | No: query semantics and evidence differ | CBPIR empirical; BF underlying-FHE assumption | **NOT FAIRLY COMPARABLE** |
| **Result set** | final sorted `First_K` stable IDs equal FPS oracle | final sorted `First_K` stable IDs equal same-`R` RFPS oracle | `S subseteq S_dec` and `|S_dec\S|<=16` with overwhelming probability | Yes at functional index-set level; exact and relaxed relations differ | CBPIR empirical (E); BF theorem (T) | **READY WITH FOOTNOTE** |
| Locate mapping / truncation | boundary filtering, stable-ID mapping, deduplication, sorting, `First_K` | same | COIE Decode recovers candidates; no `First_K`; `s=K` workload | Partial | CBPIR empirical; BF theorem | **NOT FAIRLY COMPARABLE** as a separate column; included in Result set |
| **Payload** | exact BGV reconstruction for selected IDs | exact BGV reconstruction for actual selected IDs, even if RFPS differs from oracle | correct PIR reconstruction for every requested index | Yes, conditional on the requested index set | CBPIR empirical (E); BF underlying-PIR assumption (A) | **READY WITH FOOTNOTE** |
| End-to-end semantic output | exact payloads for bounded literal-match IDs when threshold margin holds | exact payloads for RFPS-selected IDs; not exact-string precision | all true records plus at most 16 COIE-candidate records, after removing known dummies | Partial; output-set semantics differ | mixed theorem, empirical evidence, and assumptions | **NOT FAIRLY COMPARABLE** as one scalar |
| RFPS cancellation / threshold-only acceptance | not applicable | inherent RFPS semantics, not implementation failures | not applicable | No | CBPIR oracle classification | **CBPIR ONLY** |
| BF-COIE abort | not applicable | not applicable | abort before PIR when `e>16`; event negligible for `lambda=40` per Choi et al. | No | BF theorem/parameter statement | **BF-COIE ONLY** |
