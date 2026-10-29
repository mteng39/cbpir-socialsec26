# Methodology validation report

## Outcome

The methodology is usable for a **matched local BF-COIE estimate with stated limits**. It is not sufficient to reproduce the authors' implementation or its exact communication counts.

## Required questions

### 1. Does the paper uniquely determine the structural operation/count model?

Partly. It uniquely gives `m`, the real sufficient BF dimension, power-of-two hierarchy formula, number of levels, loop bounds, zero ciphertext-ciphertext multiplications, Decode's probabilistic bound, and `s+fp` PIR requests. It does not uniquely give integer BF size, non-power-of-two hierarchy, exact collision-dependent addition count, or Figure 5's object accounting.

### 2. Which integer conventions remain ambiguous?

Converting real `ell` by floor, nearest, or ceiling; handling non-integral `t`; whether the implementation pads `n`; and whether an encrypted-zero accumulator adds `n` or a reduction uses `n-1` additions. Exact Encode additions also depend on hash collisions because first insertion into `nil` is assignment.

### 3. Is 1,323 reconciled?

No. `05_paper_reconciliation.md` classifies it as **UNRESOLVED PAPER/IMPLEMENTATION DETAIL**. No documented hierarchy and BF-dimension convention yields 1,323.

### 4. How did the paper handle `n=10,000`?

The paper does not say. Floor gives `t=8`; ceiling or padding to 16,384 gives `t=9`. The model refuses a silent choice. This ambiguity affects paper reconciliation but not our 12 power-of-two cells.

### 5. Are all 12 cells structurally valid?

Yes. Each has power-of-two `n`, power-of-two `s`, and integral `n/(2s)` with `2s<=n`. Each is algorithm-defined. All are classified **structurally supported but outside original experimental range**, because the paper tested `n>=1,000`. The asymptotic `s=o(n)` efficiency claim needs qualification, especially at `n=128,s=16`; the finite construction remains defined.

### 6. Can SEAL 3.6.6 BFV support all three `N` values at standard 128-bit security?

Yes at the context level. An untimed source-built probe using `BFVDefault(N,tc128)` and `plain_modulus=65537` returned valid contexts, batching, `tc128`, and `N` slots at all three degrees. Total coefficient-modulus bits were 218, 438, and 881. SealPIR execution remains untested.

### 7. Is `t=65537` suitable?

It is suitable as a **matched local plaintext-modulus choice** for COIE: prime, batching-compatible at all three degrees, and safely above intended counters and byte values. It is not an authors' parameter. Historical SealPIR compatibility and its resulting expansion ratio remain a next-phase gate.

### 8. What is the exact matched record representation?

Exactly 64 bytes per frozen record. The bytes are the prefix of the existing 128-byte P0-D frame: `P0D1` magic (4), stable ID (8), payload length (2), payload (18--40 in these frozen databases), then zeros to byte 64. Meaningful prefixes are 32--54 bytes and padding is 10--32 bytes. Recovery zero-extends to 128 before the unchanged decoder.

### 9. Can dependency measurements be composed meaningfully?

Yes, if full kernels are measured for Count reduction, BF hierarchy accumulation, BF batch decryption, Decode traversal, and batches of SealPIR operations. The outputs remain Class `E` estimates, not full-protocol measurements.

### 10. Which effects require kernels?

Ciphertext allocation/copying, collision-dependent accumulator initialization, cache locality, batch decryption, BF traversal, context warmup, and SealPIR database-cache/preprocessing amortization. These must not be modeled as operation count times one scalar primitive latency.

### 11. Can exact serialized communication be modeled cleanly?

Yes for the matched local implementation after actual objects exist. Serialize the count ciphertext, clear `s`, all BF outputs, hash descriptions, and every SealPIR query/response. It cannot be derived defensibly from the paper's average bytes or 1,323 count.

### 12. Which Table 2 columns can be populated?

Eventually: estimated Locate, estimated Fetch, and separately labelled PostMatch; actual locally serialized Locate communication, Fetch communication, and post-Match total. Setup, Match, Online, and Match communication must remain `--`. Add a Locate communication column; do not hide COIE communication inside PIR Fetch.

## Structural test evidence

Command:

```text
python3 experiments/cbpir-scaling/bf-coie-estimation-methodology/04_structural_tests.py
```

Seven tests pass. They cover all 12 controlled cells, the Section 4.2 projection example, all requested invariants, integer BF candidates, exact-inequality diagnostic, explicit non-power-of-two policies, and failure to reconcile 1,323.

## Accepted exact structure for the local controlled matrix

Freeze only after review:

- `fp=16`, `lambda=40`, `eta=2`;
- `t=log2(n/(2s))`, valid exactly for all 12 cells;
- levels `t+1`;
- `m=max(2s,s+2fp)`;
- paper sufficient `ell_raw=eta*s*m^(1/eta)`;
- use `ceil(ell_raw)` as the proposed local implementation rule because it preserves the sufficient real bound when converted to an integer;
- compactness `c=(t+1)*ell`;
- Count reduction `n-1` additions under an explicit reduction convention;
- Encode attempts/hashes `eta*n*(t+1)`, with actual additions measured rather than assumed;
- Decode BF.Check upper bound `2s+t*m`;
- ciphertext-ciphertext multiplications zero;
- PIR requests `s+16`.

The ceiling proposal is a local convention. It is not attributed to Choi et al.

## Preservation

No BF-COIE protocol was implemented. No final estimates or Table 2 rows were produced. CBPIR was not rerun. Frozen CBPIR artifacts and paper sources were read only.

