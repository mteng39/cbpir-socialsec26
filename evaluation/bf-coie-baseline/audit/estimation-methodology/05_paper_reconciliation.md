# Reconciliation of Figure 5's 1,323 ciphertexts

## Published validation point

For `n=10,000`, `s=16`, `fp=16`, and `eta=2`, Figure 5 reports 1,323 FHE ciphertexts, 32 PIR requests, and two FHE ciphertexts after batching. These are Class `V` observations.

The 32 PIR requests reconcile exactly:

\[
s+fp=16+16=32.
\]

## Formula arithmetic

The paper's stated BF parameters give:

\[
m=\max(32,48)=48,
\]

\[
\ell_{raw}=2\cdot16\sqrt{48}=221.702503\ldots
\]

Candidate integer dimensions are 221 by floor, and 222 by nearest or ceiling. The smallest integer satisfying the displayed exponential false-positive inequality is 206; this is not the paper's stated sufficient construction, but it is included as a diagnostic.

`n/(2s)=312.5`, so the hierarchy is not integral. Candidate interpretations are:

- floor: `t=8`, 9 BFs;
- ceil: `t=9`, 10 BFs;
- pad `n` to 16,384: `t=9`, 10 BFs.

## Unbatched count candidates

If Figure 5 counts only Bloom-filter ciphertexts, `#ct=c=(t+1)ell`. If it also counts the encrypted Hamming-weight response, `#ct=1+c`.

| hierarchy | integer `ell` | BF only | count + BF |
|---|---:|---:|---:|
| floor `t=8` | 221 | 1,989 | 1,990 |
| floor `t=8` | 222 | 1,998 | 1,999 |
| ceil/pad `t=9` | 221 | 2,210 | 2,211 |
| ceil/pad `t=9` | 222 | 2,220 | 2,221 |
| floor `t=8`, exact-inequality minimum | 206 | 1,854 | 1,855 |
| ceil/pad `t=9`, exact-inequality minimum | 206 | 2,060 | 2,061 |

None equals 1,323.

## Reverse arithmetic

- If the encrypted count is excluded, `1323/9=147`, so nine equal BFs of dimension 147 would match arithmetically. But 147 is neither a documented rounding of 221.7025 nor sufficient for the displayed `1/48` inequality.
- If the encrypted count is included, the hierarchy would contribute 1,322 cells. This is not divisible by 9 or 10, so it cannot be equal-size BFs under the documented construction.
- Adding or removing one query-side ciphertext does not reach a documented candidate.
- Figure 5 lists PIR requests separately as `#PIR=32`; including SealPIR ciphertexts inside `#ct` would double-count across columns and does not explain 1,323.
- The paper says the table excludes LEAF+'s query and discusses FHE ciphertexts separately from PIR communication. No text supports adding a hidden query vector to the BF-COIE count.

## Batched clue

The reported batched count of two is consistent with one encrypted count plus one packed BF hierarchy. That reading makes the unbatched expectation `1+c`, but the documented equal-width formulas still cannot yield 1,323.

Possible undocumented implementation details include unequal per-level BF dimensions, a different BF sizing heuristic, transmitting only selected cells, or a non-power-of-two hierarchy layout. No recovered source establishes one of these.

## Classification

**UNRESOLVED PAPER/IMPLEMENTATION DETAIL**

The discrepancy does not prevent a local matched estimator if it uses an explicit local hierarchy/rounding convention, directly measures its kernels, and serializes actual local objects. It does prevent claiming that the structural model reproduces the authors' communication implementation or that 1,323 validates our eventual ciphertext count.

Consequently, paper-facing estimates may be reported only as matched local BF-COIE estimates with this limitation. They must not be presented as reproduction of Choi et al.'s exact implementation.

