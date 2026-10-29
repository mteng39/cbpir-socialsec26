# BF-COIE comparison

This directory contains the audited BF-COIE estimator lineage, including source
extraction, model fits, sensitivity scenarios, table-cell reproduction, and claim
verification.

## Evidence boundary

- CBPIR values are locally measured; BF-COIE values are derived estimates.
- BF-COIE was not reimplemented.
- Estimates use Choi et al.'s formulas, digitized Figure 4 values, fitted or
  extrapolated models, and paper-era SealPIR mechanics.
- BF-COIE Query and Match costs are unavailable. Estimated Online/Total values
  begin after Match and are not same-boundary end-to-end measurements.
- Communication remains model-dependent because the published Figure 5 count of
  1,323 ciphertexts is not reproduced by the published formulas.

## Main artifacts

- `08_latency_estimates.csv` — central and sensitivity latency estimates.
- `09_communication_estimates.csv` — formula-derived communication estimates.
- `12_table2_proposal.csv` — estimator table cells.
- `15_1323_ciphertext_investigation.md` — unresolved communication discrepancy.
- `21_table_cell_reproduction.csv` — independent table-cell reconstruction.
- `21_claim_verification.csv` — original claim audit.

The `21_*` chain records the submitted-paper `30.7x` calculation. Camera-ready R1.3
changed CBPIR Online to the sum of printed stage medians, producing `31.5x` while
leaving communication at `14.8x`. The current calculation is reproduced under
`../final-paper/` without altering frozen raw measurements.
