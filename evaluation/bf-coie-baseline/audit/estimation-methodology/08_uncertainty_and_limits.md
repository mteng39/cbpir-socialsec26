# Uncertainty and limits

## Provenance rule

Every eventual number must carry one or more tags:

- `P`: exact paper structure under a stated convention;
- `M`: local dependency/kernel measurement;
- `E`: composed estimate;
- `V`: published external validation point.

An `E` value must list the exact `P` counts and `M` samples used to produce it.

## Structural uncertainty

- **Original BFV parameters:** not recovered. Local SEAL parameters cannot be called the authors' parameters.
- **Bloom-filter integer dimension:** the paper gives a real sufficient formula but no conversion to an integer. Floor, nearest, and ceiling differ.
- **Non-power-of-two hierarchy:** the paper tests `n=10,000` but its algorithm gives only a power-of-two simplification. Floor, ceiling, padding, or an uneven tree are not specified.
- **Figure 5:** 1,323 FHE ciphertexts is not produced by the documented equal-width BF hierarchy under the tested conventions.
- **Exact Encode additions:** `nil` assignment makes the count depend on sampled hash collisions.

## Measurement and composition uncertainty

- Isolated BFV additions do not capture allocation, copies, cache behavior, or BF accumulator occupancy.
- Batch decryption can behave differently from `c` independent decryption timings.
- Hash and BF.Check costs depend on seed representation and memory locality.
- First-operation/context warmup must not be hidden.
- SealPIR database preparation is query-independent auxiliary work; request batching may amortize memory and preprocessing differently.
- Serialization and deserialization are separate from computation and may use compression.
- The historical software stack may need compatibility-only build patches; any semantic patch invalidates direct comparison.

## External validity

- Choi et al.'s machine differs from the CBPIR VPS. Their times validate structure and dominance only.
- Their tested `n` begins at 1,000; our `n={128,256,512}` is smaller.
- The finite algorithms remain defined for all 12 cells, but asymptotic compactness claims and empirical scaling should not be extrapolated without qualification.
- The paper used 16-bit items. The matched local plan uses CBPIR's exact 64-byte fixed record, so its SealPIR costs intentionally differ.

## Reporting limits

Allowed after the next phase:

- matched-local estimated Locate, Fetch, and PostMatch latency;
- actual local serialized Locate and Fetch communication;
- exact structural counts under a frozen local convention;
- comparison to paper values as non-calibrating sanity checks.

Unavailable or prohibited:

- reproduced authors' timings or parameters;
- BF-COIE Match, full Online, or comparable Setup;
- exact authors-implementation ciphertext count;
- final estimates if SealPIR cannot support the matched record/parameter mapping unchanged;
- claims that a composed estimate equals an end-to-end measurement.

