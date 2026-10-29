# Investigation of Figure 5's 1,323 BF-COIE ciphertexts

## Scope and evidence

This note investigates one communication-count inconsistency. It does not
change the estimator, apply a correction factor, run a benchmark, or modify the
manuscript.

Primary evidence:

- Choi et al., *Compressed Oblivious Encoding for Homomorphically Encrypted
  Search*, [arXiv:2109.07708v1](https://arxiv.org/abs/2109.07708), especially
  Algorithms 1, 2, and 7; Sections 4.2, 6.3, and 7.3; and Figure 5.
- The official [arXiv v1 source archive](https://arxiv.org/format/2109.07708v1),
  whose downloaded archive has SHA-256
  `b36a1402e73a392149c70e2fb15a779de46baa5f0feb2f6d04381f5dba1a1d4b`.
  Relevant source files are `BFindex.tex`, `ssp.tex`, and `exp.tex`.
- The independent [IACR ePrint copy](https://eprint.iacr.org/2021/1199.pdf).
- Existing local audits:
  [methodology reconciliation](../bf-coie-estimation-methodology/05_paper_reconciliation.md)
  and [source audit](../bf-coie-baseline/00_source_audit.md).

The arXiv archive contains the paper's TeX and rendered figure images. It does
not contain implementation code, raw experiment data, parameter files, or a
Figure 5 generation script. The source audit found no authentic public
BF-COIE implementation or supplementary artifact. In `exp.tex`, `1323` occurs
only as a literal table entry. The same file separately fixes the Figure 5
caption to `n=10,000` and `s=16`.

## 1. Independent reconstruction

Use the paper's stated Figure 5 parameters:

\[
n=10{,}000,\quad s=16,\quad f_p=16,\quad \eta=2.
\]

The real-valued hierarchy parameter is

\[
t_{\rm raw}=\log_2\!\left(\frac{10{,}000}{2\cdot16}\right)
=\log_2(312.5)=8.2877123795\ldots
\]

Hence `floor(t)=8`, `nearest(t)=8`, and `ceil(t)=9`. Algorithm 1 emits
levels `k=0,...,t`, so these choices mean 9 or 10 Bloom filters.

The Bloom-filter sizing terms are

\[
m=\max(2s,s+2f_p)=\max(32,48)=48,
\]

\[
\ell_{\rm raw}=\eta s m^{1/\eta}
=2\cdot16\sqrt{48}=221.7025033688\ldots
\]

Hence `floor(ell)=221`, while `nearest(ell)=ceil(ell)=222`. With one
ciphertext per Bloom-filter cell, the encrypted BF hierarchy contains

\[
c=(t+1)\ell
\]

ciphertexts. Algorithm 7 also sends one encrypted Hamming-weight result
`[[s]]` before the BF hierarchy. If Figure 5 accounts for that message, its
unbatched FHE count is `c+1`.

| `t` rule | `t` | levels | `ell` rule | `ell` | BF encoding `c` | `c+1` | distance of `c+1` from 1,323 |
|---|---:|---:|---|---:|---:|---:|---:|
| floor | 8 | 9 | floor | 221 | 1,989 | 1,990 | 667 |
| floor | 8 | 9 | nearest | 222 | 1,998 | 1,999 | 676 |
| floor | 8 | 9 | ceil | 222 | 1,998 | 1,999 | 676 |
| nearest | 8 | 9 | floor | 221 | 1,989 | 1,990 | 667 |
| nearest | 8 | 9 | nearest | 222 | 1,998 | 1,999 | 676 |
| nearest | 8 | 9 | ceil | 222 | 1,998 | 1,999 | 676 |
| ceil | 9 | 10 | floor | 221 | 2,210 | 2,211 | 888 |
| ceil | 9 | 10 | nearest | 222 | 2,220 | 2,221 | 898 |
| ceil | 9 | 10 | ceil | 222 | 2,220 | 2,221 | 898 |

Therefore 2,221 is not the only formula-derived candidate. It is specifically
the `ceil(t)`, `ceil/nearest(ell)`, and `+1 Count ciphertext` convention.
No tested floor, nearest, or ceiling combination produces 1,323.

Algorithm 1 assumes powers of two “for simplicity.” The paper gives no
integer convention for its non-power-of-two `n=10,000` experiment. Padding
`n` upward to 16,384 gives `t=9` and therefore 2,221 under ceiling `ell`.
Using the lower power 8,192 gives `t=8` and 1,999, but cannot represent all
10,000 records without another undocumented rule.

As a diagnostic only, the smallest integer `ell` satisfying the paper's
displayed exponential false-positive inequality is 206. It gives 1,855 or
2,061 including Count for 9 or 10 levels. It also does not explain 1,323 and
is not the sufficient sizing expression stated for the experiments.

## 2. Meaning of Figure 5's `#ct's`

Algorithm 7's post-Match communication is:

1. Server sends one encrypted Hamming weight `[[s]]` to Client.
2. Client returns plaintext `s`; this is not an FHE ciphertext.
3. Server sends `c` encrypted COIE values.
4. PIR follows as a separate step.

Figure 5 has separate `#ct's` and `#PIR` rows. The `#PIR=32` entry for
BF-COIE equals `s+fp=16+16`; those PIR objects are therefore not part of
the 1,323 FHE-ciphertext entry.

PS-COIE resolves the Count question. Its published compactness is exactly
`c=s=16`, yet Figure 5 reports 17 ciphertexts. The extra ciphertext is the
encrypted Count result. Thus the internally consistent reading is:

\[
\#ct_{\rm BF\mbox{-}COIE}=1+c.
\]

The encrypted search query and encrypted match vector are outside this
post-Match communication row. The plaintext return of `s` is communication,
but not an FHE ciphertext and therefore not counted in `#ct's`.

BFS-CODE provides a second warning that Figure 5 does not transparently follow
the printed sizing formulas. Section 7 sets `lambda=40`, `s=16`,
`eta=lambda+log2(s)=44`, and `ell=2(eta*s-1)=1,406`. Algorithm 7's CODE
variant would therefore suggest 1,407 ciphertexts including Count, whereas
Figure 5 reports 1,321. This does not solve BF-COIE's discrepancy. It shows
that BF-COIE is not the only row whose printed parameters and table count do
not directly reconcile.

## 3. Hypotheses for 1,323

| hypothesis | `t` / levels | `ell` | encoding ciphertexts | extra ciphertexts | total | distance from 1,323 | evidence |
|---|---:|---:|---:|---:|---:|---:|---|
| Printed formula, ceiling hierarchy and BF size | 9 / 10 | 222 | 2,220 | 1 Count | 2,221 | 898 | Direct formulas; conservative non-power-of-two convention |
| Printed formula, floor hierarchy and BF size | 8 / 9 | 221 | 1,989 | 1 Count | 1,990 | 667 | Direct formulas; lower rounding convention |
| Pad `n` to 16,384 | 9 / 10 | 222 | 2,220 | 1 Count | 2,221 | 898 | Plausible power-of-two handling, but undocumented |
| Use lower power 8,192 | 8 / 9 | 222 | 1,998 | 1 Count | 1,999 | 676 | Cannot cover all 10,000 records without an undocumented layout |
| Exact-inequality minimum | 8 / 9 | 206 | 1,854 | 1 Count | 1,855 | 532 | Mathematical diagnostic; not stated experiment rule |
| Figure actually used `n=1,000`, floor `ell` | 5 / 6 | 221 | 1,326 | 1 Count | 1,327 | 4 | Numerically close; caption and prose contradict it |
| Figure actually used `n=1,000`, ceiling `ell` | 5 / 6 | 222 | 1,332 | 1 Count | 1,333 | 10 | `1,332` resembles a digit transposition only if Count is omitted |
| Nine filters of size 147, Count omitted | 8 / 9 | 147 | 1,323 | 0 | 1,323 | 0 | Arithmetic only; conflicts with BF sizing and PS-COIE accounting |
| Count included, exact total 1,323 | 9 or 10 levels | unequal/unknown | 1,322 | 1 Count | 1,323 | 0 | Requires unequal sizes or omitted cells; Algorithm 1 specifies equal `ell` |
| Pre-existing ciphertext packing | any | any | packed | 1 Count | 2 expected | 1,321 | Contradicted by the separate “with batching” row |

Changing only the effective sparsity upper bound also fails: no integer
`s` from 1 through 64, combined with the standard floor/nearest/ceiling rules,
produces 1,323 from the published formulas at `n=10,000`.

Potential implementation choices—unequal per-level BF sizes, omitted levels,
merged levels, or a separate sizing heuristic—could produce another total.
They remain hypotheses. Neither the paper nor recovered source documents such
a choice, and none has documentary support sufficient to reconstruct 1,323.

## 4. The `n=1,000` and `1,332` hypotheses

For `n=1,000`, the BF size remains
`ell_raw=221.702503...`, while

\[
t_{\rm raw}=\log_2(1000/32)=4.9657842847\ldots
\]

The complete rounding results are:

| `t` rule | levels | `ell` rule | `ell` | BF encoding `c` | `c+1` |
|---|---:|---|---:|---:|---:|
| floor | 5 | floor | 221 | 1,105 | 1,106 |
| floor | 5 | nearest/ceil | 222 | 1,110 | 1,111 |
| nearest/ceil | 6 | floor | 221 | 1,326 | 1,327 |
| nearest/ceil | 6 | nearest/ceil | 222 | 1,332 | 1,333 |

Thus `n=1,000` is suggestive but does not reconstruct 1,323. The nearest
Count-inclusive result is 1,327. The value 1,332 is an exact six-level BF-only
count and is a transposition of the digits in 1,323, but Figure 5's PS-COIE
row indicates Count should be included, making the corresponding total 1,333.

Documentary evidence does not support the workload-switch hypothesis:

- Figure 5's TeX caption explicitly says `n=10,000` and `s=16`.
- The surrounding communication prose repeats `n=10,000` and `s=16`.
- The result-size experiment immediately before it also fixes `n=10,000`.
- The paper states only that the overall experiment range begins at
  `n=1,000`; it does not connect Figure 5 to that endpoint.
- No raw-data file or figure script was present to reveal a mislabeled input.

Numerical resemblance alone is insufficient to classify this as a probable
`n=1,000` label error or a probable `1332 -> 1323` transposition.

## 5. Batching cannot explain 1,323

Figure 5 explicitly labels 1,323 in the unbatched `#ct's` row and 2 in the
separate `#ct's (w/ batching)` row. Section 7.3 says the optimization packs
the protocol's ciphertext vector into one ciphertext, while the encrypted
Count remains the other message. This matches the PS-COIE change from 17 to 2.

Therefore 1,323 is intended as an unbatched count: one ciphertext per encoded
value plus the Count ciphertext. There is no evidence of lower-level packing
already hidden inside 1,323. PIR is unchanged by this batching and remains in
the separate `#PIR` row.

## 6. Source-material result

Searches of the official arXiv source for `1323`, `1322`, `1332`, `2220`,
`2221`, `n=10000`, and `n=1000` found:

- one hard-coded occurrence of `1323`, in the Figure 5 table;
- no derivation of 1,323 or 1,322;
- no occurrence of 1,332, 2,220, or 2,221 as communication calculations;
- repeated prose/caption support for `n=10,000`;
- no experiment script, configuration file, or raw communication data.

No public repository or supplementary artifact recovered by the prior source
audit explains the count. The official source therefore confirms the
publication's displayed inputs and output but not the arithmetic between them.

## 7. Classification

**D. UNRESOLVED.**

No documented rounding, hierarchy, sizing, message-accounting, or batching
rule reconstructs 1,323. A typo or an undocumented implementation-specific
parameter is possible, but evidence is too weak to promote either to
“probable.” The second mismatch in the BFS-CODE row increases concern about
Figure 5's formula-to-table traceability, but does not identify the BF-COIE
cause.

## 8. Consequence for the estimator

The ratio `1323/2221` has no established structural meaning. Its denominator
is only one rounding convention, and its numerator has no recovered derivation.
Nothing shows that this ratio remains constant across `n` or `s`. It must not
be transferred to the target workloads and must be removed from the estimator
in a later, separately approved correction.

For target cells `n in {128,256,512}` and `s in {1,4,8,16}`, all
`n/(2s)` ratios are powers of two. Their hierarchy depth is therefore exact;
only conversion of the real BF dimension to an integer remains ambiguous. The
defensible protocol-derived rule is:

\[
L_{\rm BF}=\log_2(n/(2s))+1,
\]

\[
m=\max(2s,s+32),\qquad
\ell_{\rm int}=\left\lceil 2s\sqrt{m}\right\rceil,
\]

\[
\#ct_{\rm Locate}=1+L_{\rm BF}\ell_{\rm int}.
\]

The leading 1 is the encrypted Count message. Ceiling is the conservative
integer realization of the paper's sufficient real-valued BF size. A
floor-versus-ceiling count should be retained as rounding sensitivity, not as
an empirical correction. Any resulting communication value must be labeled
“published-formula-derived estimate; Figure 5's 1,323 count is unresolved,”
not an authors-implementation measurement.

This rule is suitable for a formula-derived main-table estimate only with that
explicit caveat and sensitivity. If the table requires a count demonstrably
matching Choi et al.'s implementation, BF-COIE Locate communication must remain
unavailable. The investigation provides no basis for a more precise
implementation-matched count.

## Concise decision summary

1. **Independent count:** candidates are 1,990, 1,999, 2,211, or 2,221
   including Count under the requested rounding rules; none is 1,323.
2. **2,221 assumptions:** `ceil(t)=9`, 10 levels, `ceil(ell)=222`, one
   ciphertext per BF cell, plus one encrypted Count ciphertext.
3. **Count included:** yes. PS-COIE has `c=s=16` but Figure 5 reports 17.
4. **Best reconstruction of 1,323:** none supported. Exact arithmetic needs
   either nine filters of undocumented size 147 while omitting Count, or an
   undocumented unequal hierarchy totaling 1,322 cells plus Count.
5. **`n=1,000` hypothesis:** close but not exact; documented rounding gives
   1,327 or 1,333 with Count, and source text consistently says `n=10,000`.
6. **`1332/1323` typo hypothesis:** 1,332 equals six filters of size 222 at
   `n=1,000`, but Count would make 1,333. No source evidence supports a
   transposition.
7. **Batching:** cannot explain the discrepancy. The paper separately reports
   1,323 unbatched and 2 batched.
8. **Source artifact:** none explains it. Official TeX hard-codes 1,323 but
   includes no derivation, raw data, parameter file, or script.
9. **Classification:** **D. UNRESOLVED**.
10. **Target-workload rule:** use `1 + (t+1)ceil(2s*sqrt(max(2s,s+32)))`,
    with floor/ceiling rounding sensitivity and an explicit Figure 5 caveat.
11. **Correction factor:** yes, `1323/2221` must be removed; it has no
    defensible transfer rule across `n` and `s`.
