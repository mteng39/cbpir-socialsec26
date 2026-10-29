# Final fixed-window CBPIR configuration freeze

## Scope

This is the final capacity/correctness gate before the formal seven-procedure benchmark. Diagnostic timings emitted by the runner are not paper benchmark measurements.

Fetch retains uniform fixed windows `J_i=[i*ell,(i+1)*ell)`, the unchanged 128-byte frame encoder/decoder, one encrypted BGV query, one encrypted BGV response, ciphertext-plaintext evaluation, and no physical dummy windows. The meaningful encoded prefix is measured only to choose `ell`; no variable-width layout exists.

## Complete eligible BGL pool

The source contains 1,331 deterministic eligible records. Searchable and payload lengths are both 17–40 bytes, mean 25.040571, median 21. The actual retained encoder produced meaningful prefixes of 31–54 slots, mean 39.040571, median 35.

Every one of the 1,331 records was passed through the real `encode_frame()` implementation. Embedded length equaled payload length, `required_slots=14+payload_length`, all remaining legacy bytes were zero, and zero-extension followed by unchanged `decode_frame()` reconstructed exactly. Therefore every eligible record fits `w=64`; all larger allowed widths also fit.

## Allowed-width audit

| N | w | floor(N/w) | max feasible n | max encoded slots | |T| | L | U_Match | U_Fetch | limit |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---|
| 8192 | 64 | 128 | 128 | 54 | 3201 | 4096 | 1.000000 | 1.000000 | Fetch |
| 8192 | 128 | 64 | 64 | 54 | 1629 | 2048 | 0.500000 | 1.000000 | Fetch |
| 8192 | 224 | 36 | 36 | 54 | 925 | 1024 | 0.250000 | 0.984375 | Fetch |
| 8192 | 256 | 32 | 32 | 54 | 829 | 1024 | 0.250000 | 1.000000 | Fetch |
| 8192 | 384 | 21 | 21 | 54 | 547 | 1024 | 0.250000 | 0.984375 | Fetch |
| 8192 | 512 | 16 | 16 | 54 | 416 | 512 | 0.125000 | 1.000000 | Fetch |
| 16384 | 64 | 256 | 256 | 54 | 6544 | 8192 | 1.000000 | 1.000000 | Fetch |
| 16384 | 128 | 128 | 128 | 54 | 3201 | 4096 | 0.500000 | 1.000000 | Fetch |
| 16384 | 224 | 73 | 73 | 54 | 1845 | 2048 | 0.250000 | 0.998047 | Fetch |
| 16384 | 256 | 64 | 64 | 54 | 1629 | 2048 | 0.250000 | 1.000000 | Fetch |
| 16384 | 384 | 42 | 42 | 54 | 1098 | 2048 | 0.250000 | 0.984375 | Fetch |
| 16384 | 512 | 32 | 32 | 54 | 829 | 1024 | 0.125000 | 1.000000 | Fetch |
| 32768 | 64 | 512 | 512 | 54 | 13358 | 16384 | 1.000000 | 1.000000 | Fetch |
| 32768 | 128 | 256 | 256 | 54 | 6544 | 8192 | 0.500000 | 1.000000 | Fetch |
| 32768 | 224 | 146 | 146 | 54 | 3669 | 4096 | 0.250000 | 0.998047 | Fetch |
| 32768 | 256 | 128 | 128 | 54 | 3201 | 4096 | 0.250000 | 1.000000 | Fetch |
| 32768 | 384 | 85 | 85 | 54 | 2145 | 4096 | 0.250000 | 0.996094 | Fetch |
| 32768 | 512 | 64 | 64 | 54 | 1629 | 2048 | 0.125000 | 1.000000 | Fetch |

Widths 224 and 384 use `floor(N/w)` complete fixed windows and leave a zero ring tail; exact divisibility is not required by the construction.

## Frozen databases

| Profile | N | n | ell | |T| | L | U_Match | U_Fetch | limiting stage |
|:---|---:|---:|---:|---:|---:|---:|---:|:---|
| CBPIR-13 | 8192 | 128 | 64 | 3201 | 4096 | 1.000 | 1.000 | Fetch |
| CBPIR-14 | 16384 | 256 | 64 | 6544 | 8192 | 1.000 | 1.000 | Fetch |
| CBPIR-15 | 32768 | 512 | 64 | 13358 | 16384 | 1.000 | 1.000 | Fetch |

The smallest-valid-width rule selects `ell=64` for every profile. Actual nested-prefix measurement selects `n=128`, `256`, and `512`. Both utilization ratios are exactly 1.0 in all three configurations: `L=N_CKKS/2` and `n*ell=N_BGV`. The immediately following prefix is rejected by Fetch because another 64-slot window does not fit. Match still has byte-level slack inside its already-maximal power-of-two transform, so the limiting stage is reported as Fetch rather than jointly Match+Fetch.

Payload-byte utilization is separate from view utilization. Records remain variable length and are zero-padded inside their uniform 64-slot windows.

## Match, Locate, and Fetch correctness

The same frozen database is used by FPS and RFPS for each N. FPS retained zero decision mismatches against the plaintext oracle. RFPS structural cancellation, nonzero threshold-only acceptance, and CKKS decision disagreement remain separate:

| N | RFPS cancellations | RFPS threshold-only | RFPS CKKS disagreements |
|---:|---:|---:|---:|
| 8192 | 3 | 2 | 2 |
| 16384 | 36 | 18 | 15 |
| 32768 | 74 | 48 | 43 |

The regenerated corpus retains literal, absent, repeated, boundary, fewer-than-K, exactly-K, more-than-K, RFPS literal, cancellation, and threshold-only cases. Locate alone performs deterministic First_K truncation.

Direct Fetch probes exercised realized `k=0..17`. Fetch never silently truncated, including `k=17` relative to public K=16. Every case used one encrypted query and one encrypted response. Sizes were constant within each N. Selected payloads reconstructed exactly; unselected windows and unused ring tails were zero. The empty selector was encrypted normally and decrypted to all zero.

Exact Fetch reconstruction proves reconstruction of selected IDs, not semantic RFPS correctness.

## Retained cryptographic profiles

All contexts use `HEStd_128_classic`. FPS-13 retains `(N=8192, depth=2, b0=60, bs=50, FIXEDMANUAL, BV)`; FPS-14 retains `(N=16384, depth=2, b0=60, bs=40, FIXEDMANUAL, HYBRID)`; FPS-15 retains `(N=32768, depth=2, b0=60, bs=50, FIXEDMANUAL, HYBRID)`. RFPS retains depth 1, b0 60, bs 24. BGV-RNS retains t=65537 and depth 1. Only the public fixed record-window width changes to 64.

## Evidence preservation and next gate

Original `phase2-full-capacity` artifacts remained byte-for-byte unchanged: `true`.

These three databases are frozen inputs for the later formal benchmark. That benchmark was not started here.
