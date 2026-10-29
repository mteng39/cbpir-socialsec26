# BF-COIE structural protocol model

## Boundary and rounds

The input is `n` BFV ciphertexts encrypting a match-indicator vector with at most `s` nonzero positions. Match is not modeled.

The post-Match structure from Algorithm 7 is:

1. Server reduces the encrypted indicators to an encrypted Hamming weight. Client decrypts it and returns plaintext `s`.
2. Server runs BF-COIE.Encode. Client decrypts the Bloom-filter hierarchy and runs BF-COIE.Decode.
3. Client pads the recovered index set to `s+fp` indices and performs that many SealPIR requests.

This is three interaction rounds at the paper's abstraction level.

## Paper-derived formulas

For the power-of-two case used in our controlled matrix:

\[
t=\log_2\!\left(\frac{n}{2s}\right),\qquad
L_{BF}=t+1.
\]

Here `L_BF` is the number of Bloom-filter levels and must not be confused with CBPIR's DFT length.

With `fp=16`, `lambda=40`, and `eta=2`:

\[
m=\max(2s,s+2fp),
\]

\[
\ell_{raw}=\eta s m^{1/\eta},
\qquad
c=(t+1)\ell.
\]

The real-valued `ell_raw` is a sufficient dimension from the paper's inequality. A concrete implementation still needs an integer rule. The model exposes `floor`, round-half-up `nearest`, and `ceil`; it does not pretend that the paper selects one.

The exact displayed false-positive inequality is:

\[
\left(1-e^{-\eta s/\ell}\right)^\eta\leq\frac1m.
\]

The model also reports the smallest integer satisfying this inequality. That number is an analytical comparison, not a replacement for the paper's sufficient formula.

## Structural counts

- Count reduction: `n-1` ciphertext additions for a reduction tree or chain. An encrypted-zero accumulator convention would use `n`; both are recorded.
- Algorithm 1 hash evaluations and accumulation attempts: `eta*n*(t+1)`.
- Ciphertext-ciphertext multiplications: zero.
- Encrypted Bloom-filter outputs and COIE decryptions: `c` if each cell is transmitted separately.
- Decode BF.Check upper bound under the paper's high-probability argument: `2s + t*m`.
- PIR requests: `s+fp`.

Algorithm 1 initializes cells as `nil`: a first insertion assigns a ciphertext without an addition. Therefore its exact addition count depends on hash collisions and occupied cells. The model reports the defensible range:

\[
\eta n(t+1)-c\leq A_{encode}\leq\eta n(t+1).
\]

It does not mislabel the upper bound as an exact count.

## Controlled matrix hierarchy

| `n` | `s=1` | `s=4` | `s=8` | `s=16` |
|---:|---:|---:|---:|---:|
| 128 | `t=6`, 7 levels | `t=4`, 5 | `t=3`, 4 | `t=2`, 3 |
| 256 | `t=7`, 8 | `t=5`, 6 | `t=4`, 5 | `t=3`, 4 |
| 512 | `t=8`, 9 | `t=6`, 7 | `t=5`, 6 | `t=4`, 5 |

All cells satisfy the algorithm's finite condition `2s<=n`. All are below the paper's tested `n` range of 1,000--100,000. They are algorithm-defined but outside the original experimental range. The asymptotic statement `s=o(n)` supports compactness claims; it is not a parser rule that makes these finite cells undefined.

## Non-power-of-two inputs

Algorithm 1 explicitly assumes power-of-two `n` and `s` for simplicity. For `n=10,000,s=16`, `log2(n/(2s))` is about 8.288, not an integer. The paper does not state whether its implementation floors, ceils, pads, or uses unequal top ranges. The model rejects this case unless the caller explicitly chooses `floor`, `ceil`, or `pad_to_power_of_two`.

## Section 4.2 example

For selected indices `{1,15,16}`, the implemented projection `ceil(i/2^k)` reproduces the paper's illustrative sets:

`I^4={1}`, `I^3={1,2}`, `I^2={1,4}`, `I^1={1,8}`, and `I^0={1,15,16}`.

This example illustrates the tree projection. Its three selected indices are not a complete parameter instantiation of Algorithm 1's power-of-two sparsity assumption.

