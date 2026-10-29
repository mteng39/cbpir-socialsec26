# BF-COIE Query+Match estimability audit

## Scope and result

This audit asks whether published evidence can supply the BF-COIE stage that functionally maps to CBPIR `Search`. It does not alter existing estimates or the manuscript.

The result is negative for the complete stage. Choi et al. provide an author-stated assumption that permits a conditional estimate of server `Match`, but neither Choi et al. nor LEAF report the client Query-encryption time needed for `Query+Match`. A conditional query-size estimate is possible only by imposing LEAF's 16-bit, bitwise exact-match instantiation on BF-COIE. That query is not specified as the evaluated BF-COIE query. Therefore BF-COIE `Search` and full `Online` remain unavailable for the main table.

## 1. Functional boundary and evidence classification

CBPIR defines

\[
T_{Search}=T_{SearchQuery}+T_{SearchResponse},
\]

where `SearchQuery` includes client query construction, transforms, plaintext encoding, and CKKS encryption, and `SearchResponse` includes server homomorphic evaluation. Choi Algorithm 7 step 2 sends an abstract encrypted query `[[q]]`; step 3 evaluates it against every encrypted record and leaves the encrypted match vector `[[b]]` at the server. No response message occurs between steps 2 and 3 because `[[b]]` is consumed server-side by step 4.

| quantity | evidence classification | reason |
|---|---|---|
| BF-COIE Query computation | **NOT ESTIMABLE** | Choi specifies neither the concrete query encoding nor query-encryption timing. LEAF gives a possible 16-bit exact-match encoding but no query-encryption timing. |
| BF-COIE Query communication | **ESTIMABLE ONLY WITH NEW ASSUMPTION** | A LEAF-style 16-bit, bitwise-encrypted exact-match query would contain 16 ciphertexts. Choi does not state that its BF-COIE evaluation used this query representation. |
| BF-COIE Match computation | **ESTIMABLE WITH AUTHOR-STATED ASSUMPTION** | Choi explicitly assumes `MT(LEAF) ~= MT(BF-COIE)`. LEAF Figure 9 permits extraction of `MT(LEAF)`, but the transfer crosses machines, library versions, and implementations. |
| BF-COIE Match communication | **DERIVABLE FROM SOURCE** | Match produces `[[b]]` at the server and sends no message. Query communication precedes it; Count communication begins in Algorithm 7 step 4. |

No Query or Match quantity is directly reported for Choi's BF-COIE implementation.

## 2. Choi Section 7.2 reconstructed

Choi et al. use:

- `MT(LEAF)`: one execution of LEAF's record-wise Match stage;
- `FT(LEAF)`: one execution of LEAF's first-result searching/fetching stage;
- `MT(BF-COIE)`: BF-COIE's one record-wise Match execution producing `[[b]]`;
- `FT(BF-COIE)`: BF-COIE's complete native Fetch after `[[b]]`, including Count, COIE Encode/Decode, and padded PIR.

Their equations are

\[
T(LEAF)=s\,MT(LEAF)+s\,FT(LEAF)
\]

and

\[
T(BF\text{-}COIE)=MT(BF\text{-}COIE)+FT(BF\text{-}COIE).
\]

The paper then explicitly assumes

\[
MT(LEAF)\approx MT(BF\text{-}COIE)
\]

and reads from LEAF Figure 9

\[
m=\frac{MT(LEAF)}{FT(LEAF)}\approx1.5.
\]

Finally it sets

\[
FT(LEAF)=1800\,FT(BF\text{-}COIE)
\]

for `s=16`, giving

\[
\frac{T(LEAF)}{T(BF\text{-}COIE)}
=\frac{s(m+1)}{m+1/1800}
\approx26.66,
\]

reported as roughly `26x`.

### Meaning of the `1800x` term

Section 7.1 obtains `1800x` from Choi's own LEAF+ and BF-COIE Fetch measurements:

- one LEAF+ record: `1872 s`;
- 16 sequential LEAF+ records: `16 * 1872 = 29952 s`;
- one BF-COIE Fetch recovering 16 records: `16.7 s`;
- ratio: `29952 / 16.7 = 1793.5`, rounded to `1800`.

This ratio compares **all 16 sequential LEAF+ fetches** with **one all-results BF-COIE Fetch**. In the Section 7.2 equation, however, `FT(LEAF)` is already multiplied by `s`, so it denotes one LEAF fetch. The consistent single-fetch ratio would be `1872 / 16.7 = 112.1`, not `1800`. If `FT(LEAF)` instead denotes the all-16 total, the numerator `s * FT(LEAF)` counts the 16 fetches twice.

The `1800x` substitution is therefore dimensionally seconds-over-seconds but semantically inconsistent with the variables in the algebra. Replacing `1800` by `112.1` changes the algebraic result only slightly (`26.66x` to `26.51x`) because the assumed Match term dominates the denominator. This numerical insensitivity does not repair the definition mismatch.

## 3. LEAF Figure 9 reconstruction

### What Figure 9 measures

LEAF Section 7 states that Figure 9 reports **server running time**. Figure 9(a) is the LEAF searching/fetching algorithm without Match. Figure 9(b) is searching/fetching plus Match. Client query generation, encryption, communication, and client decryption are not included. Therefore

\[
MT_{LEAF}(n)=T_{9(b)}(n)-T_{9(a)}(n).
\]

The experiment uses Microsoft SEAL 3.4.5, BFV, no batching optimization, error rate `2^-80`, and exact-match keyword length `mu=16` bits.

### Digitized values

The embedded Figure 9 plots are vector graphics. Marker coordinates and axis transforms were extracted directly from the local published PDF. Values remain figure-extracted approximations because raw measurements, repetitions, and error bars are unavailable. Coordinate-conversion error is below about `0.1 s`; unreported experimental variance is unknown.

| `n` | Figure 9(a), search/fetch only (s) | Figure 9(b), search/fetch + Match (s) | derived Match (s) | `Match/Fetch` |
|---:|---:|---:|---:|---:|
| 36 | 0.827 | 1.253 | 0.426 | 0.515 |
| 64 | 1.184 | 1.936 | 0.752 | 0.636 |
| 100 | 1.540 | 2.724 | 1.184 | 0.769 |
| 225 | 2.457 | 5.088 | 2.632 | 1.071 |
| 400 | 3.628 | 8.293 | 4.665 | 1.286 |
| 900 | 6.530 | 17.014 | 10.484 | 1.606 |
| 2500 | 15.543 | 44.596 | 29.054 | 1.869 |
| 6400 | 41.358 | 115.785 | 74.426 | 1.800 |
| 10000 | 68.549 | 185.134 | 116.585 | 1.701 |

At `n=10000`, the reconstructed ratio is about `1.70`, not exactly `1.5`. Across the plotted range the ratio is not constant. Choi's `m ~= 1.5` is a coarse reading, not a fitted invariant.

### Exact-match operation model and fit

LEAF defines, for `mu`-bit values,

\[
IsEqual_{\mu}(a,b)=\prod_{i=1}^{\mu}(1+a_i+b_i)\pmod 2.
\]

The stated circuit uses `mu-1` multiplications per record; the paper later summarizes whole-database Match as approximately `mu*n` multiplications. With fixed `mu=16`, both descriptions imply linear-through-origin scaling in `n`.

Fitting the nine derived Match values gives:

\[
\widehat{MT}_{LEAF}^{origin}(n)=0.0116488n\ \text{seconds},
\qquad RMSE=0.0583\ \text{seconds}.
\]

An intercept fit gives

\[
\widehat{MT}_{LEAF}^{intercept}(n)=0.0116499n-0.00789\ \text{seconds},
\qquad RMSE=0.0579\ \text{seconds}.
\]

The intercept gives no meaningful fit improvement and is slightly negative. The through-origin model is therefore both structurally and empirically defensible for LEAF's Match.

| target `n` | through-origin Match (s) | local linear interpolation (s) | difference |
|---:|---:|---:|---:|
| 128 | 1.491 | 1.508 | 0.017 |
| 256 | 2.982 | 2.992 | 0.010 |
| 512 | 5.964 | 5.968 | 0.004 |

All three target sizes lie **inside** Figure 9's plotted `n=36..10000` range. Estimating LEAF Match at these `n` values is interpolation, not downward extrapolation. Transferring those values to BF-COIE remains a separate cross-implementation assumption.

The Figure 9 ratio `m` interpolates to roughly `0.86`, `1.12`, and `1.40` at `n=128`, `256`, and `512`. Applying Choi's global `m=1.5` blindly to these small workloads is weaker than using the directly extracted Match curve.

## 4. Machine and implementation transfer

| property | LEAF Figure 9 | Choi BF-COIE evaluation |
|---|---|---|
| CPU | Intel Xeon E7-8867 v3 @ 2.50 GHz | Intel Core i9-9900K @ 4.7 GHz |
| RAM | 1,536 GB | 64 GB |
| library | Microsoft SEAL 3.4.5, BFV | PySEAL / Microsoft SEAL 3.6, BFV |
| batching | none | none for computation |
| reported timing | server time | post-Match Fetch time |
| thread evidence | LEAF paper does not state a one-thread limit | one thread |

Three transfer methods have different status:

1. **Absolute LEAF Match time assigned to BF-COIE.** Not defensible as a measured BF-COIE time. No cross-machine or cross-version calibration exists.
2. **LEAF internal ratio only.** This is the method Choi uses. It avoids directly asserting identical seconds, but `m` varies with `n` and does not supply absolute BF-COIE Match time by itself.
3. **LEAF ratio combined with Choi Fetch.** This reproduces Choi's Section 7.2 style, but the `1800x` variable mismatch prevents a clean absolute calibration. It supports only a coarse sensitivity estimate.

Thus the extracted `0.0116488n` seconds model is valid evidence for LEAF's server Match. Calling it BF-COIE Match requires Choi's explicit equality assumption and a strong cross-environment qualification. It is not a hardware-calibrated BF-COIE estimate.

## 5. Query computation and communication

Choi Algorithm 7 says only that the client sends encrypted `[[q]]`. It does not instantiate the query circuit, state a ciphertext count, report encryption time, or report serialized query bytes.

LEAF supplies one possible instantiation:

- exact equality on a `mu=16`-bit keyword;
- vectors are encrypted bit by bit under the paper's FHE notation;
- no batching optimization is used in the Figure 9 comparison.

Under the **new assumption** that Choi BF-COIE uses this exact LEAF query representation, Query contains 16 ciphertexts. Combining that assumption with Choi's approximate `103 KB` per FHE ciphertext gives approximately `1,648 KB` (`1.57 MiB`) client-to-server. This is only a sensitivity construction: Choi's `103 KB` value describes its reported FHE objects generally, but the paper does not confirm the concrete query encoding, query ciphertext level, or serialization size.

No paper value gives the time to create and encrypt those 16 ciphertexts. Query computation is therefore not estimable even under the conditional 16-ciphertext representation.

Match communication after Query is exactly zero at the audited boundary. Algorithm 7 leaves `[[b]]` at the server. The next message is the encrypted count in step 4, which belongs to comparison-layer Locate.

## 6. Semantic comparability

Verdict: **NOT SUFFICIENTLY COMPARABLE** for a main-table Search number.

The functional boundaries are close: both stages begin with a private request and end with protected match evidence. The query workloads are not matched:

- CBPIR searches variable-length patterns over concatenated searchable fields and returns encrypted alignment scores;
- LEAF Figure 9 evaluates 16-bit exact equality independently over records and returns encrypted match bits;
- CBPIR supports FPS and randomized RFPS semantics; LEAF's calibration covers neither;
- matching database cardinality `n` does not match searched text volume, pattern length, circuit depth, or number of encrypted query terms.

Functional alignment therefore does not justify treating LEAF exact-match seconds as directly comparable CBPIR Search seconds.

## 7. Search and Online recoverability

| aggregate | computation status | communication status | reason |
|---|---|---|---|
| BF-COIE `Search = Query+Match` | not defensible | sensitivity only | Match has a conditional model, but Query computation is missing. Query bytes require an unverified 16-ciphertext instantiation. |
| BF-COIE `Online = Search+Locate+Fetch` | not defensible | not defensible | Existing Locate/Fetch estimates do not supply the missing Query computation or a main-table-ready Search workload. |

Adding a Match-only estimate to existing Locate and Fetch values would not produce CBPIR-aligned Online latency. It would omit client Query work and mix a LEAF 16-bit exact-match server measurement with Choi post-Match estimates and CBPIR pattern-search semantics.

## 8. Decision table

| Quantity | Evidence | Proposed model | Confidence | Main-table status |
|---|---|---|---|---|
| BF-COIE Query computation | Algorithm 7 step 2 gives no concrete encoding or timing | none | high confidence in absence | **NOT DEFENSIBLE** |
| BF-COIE Query communication | conditional LEAF 16-bit bitwise query; Choi approximate ciphertext size | `16` ciphertexts, about `1.57 MiB`, only if LEAF exact-match representation is imposed | low | **SENSITIVITY ONLY** |
| BF-COIE Match computation | LEAF Figure 9 difference; Choi explicitly assumes equal Match time | `0.0116488n s`: `1.491`, `2.982`, `5.964 s` for target `n`; cross-machine and semantic sensitivity only | low-to-medium | **SENSITIVITY ONLY** |
| BF-COIE Search computation | requires Query computation plus Match | none; Query computation missing | high confidence in absence | **NOT DEFENSIBLE** |
| BF-COIE Search communication | Match sends nothing; Query size conditional | conditional 16-ciphertext query only | low | **SENSITIVITY ONLY** |
| BF-COIE Online computation | requires complete Search plus existing Locate and Fetch | none; Match alone cannot replace Search | high confidence in absence | **NOT DEFENSIBLE** |
| BF-COIE Online communication | requires main-table-ready Search communication plus Locate and Fetch | none; only a conditional query-size sensitivity exists | high confidence in absence | **NOT DEFENSIBLE** |

## 9. Exact answers

1. **Can Match computation be estimated?** Yes, conditionally. LEAF Figure 9 supports `0.0116488n s`; assigning it to BF-COIE uses Choi's explicit assumption and must remain sensitivity-only.
2. **Can Query computation be estimated?** No. Neither paper reports query-encryption time or enough primitive timing data to derive it.
3. **Can `Search = Query+Match` be estimated?** No. Query computation is missing, and the available Match calibration uses different query semantics.
4. **Can Search communication be estimated?** Not for the main table. A conditional LEAF exact-match representation gives 16 ciphertexts, about `1.57 MiB`, but this is a new assumption and sensitivity only.
5. **Can full Online computation be estimated?** No. A defensible Search computation is absent.
6. **Can full Online communication be estimated?** No. Search communication is not established for Choi's evaluated BF-COIE query.
7. **If Match alone is estimable, should it appear in the main table?** No. The table's common column is Search, not Match, and LEAF's 16-bit equality workload is not sufficiently comparable to CBPIR pattern search.
8. **Should the final table retain an Online column?** It may remain for CBPIR continuity, but BF-COIE Online must stay `--`. `Locate+Fetch` remains the common populated aggregate.
9. **Does any new estimate require modifying existing Locate/Fetch values?** No. This audit changes no accepted Locate or Fetch model or value.
