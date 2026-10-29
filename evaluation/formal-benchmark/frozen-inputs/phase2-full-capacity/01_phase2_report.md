# Phase 2 — Full-capacity CBPIR configuration selection

## Scope

This is a bounded configuration/correctness gate. Timings are diagnostic only. It is not the later formal seven-procedure benchmark.

The Match backend is explicit `server_text_ct_pt`. Fetch uses the existing P0-A multi-window BGV selector directly. P3 remains unchanged. Empty selection is encrypted as an all-zero selector and still produces one encrypted query and one encrypted response. No physical dummy record windows exist.

## Frozen databases

| N | n | |T| | L | U_Match | payload bytes | ell | U_Fetch | limiting stage |
|---:|---:|---:|---:|---:|---:|---:|---:|:---|
| 8192 | 64 | 1629 | 2048 | 0.500 | 1566 | 128 | 1.000 | Fetch |
| 16384 | 128 | 3201 | 4096 | 0.500 | 3074 | 128 | 1.000 | Fetch |
| 32768 | 256 | 6544 | 8192 | 0.500 | 6289 | 128 | 1.000 | Fetch |

Source: Loghub BGL_2k at `dd61d0952749ee7963bde24220d1be5ede023033`; SHA-256 `2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496`. The retained seed `p0d-bgl-k1-20260911-v1` ranks 1,331 eligible distinct source lines. Configurations are nested prefixes of 64, 128, and 256 records, then ordered by stable source-line RecordId.

Searchable fields and payloads are the real selected message suffixes. Neither is padded to inflate utilization. The zero bytes between searchable fields are structural guards, not records.

## Query corpus

There are 51 profile-specific query rows (17 per database). Pattern length m ranges from 1 to 16 bytes; mean m is 4.078. K is exercised separately at `{1,4,8,16}` and is not used in the database-capacity calculation.

The deterministic corpus includes literal FPS matches, absence, repeated occurrences, guard-crossing matches, and fewer-than/exactly/more-than-K candidate sets. RFPS cases include a literal match, exact signed-score cancellation, and nonzero threshold-only acceptance.

## Correctness

| N | FPS queries | FPS max error | FPS decision mismatches | RFPS cancellations | RFPS threshold-only | RFPS CKKS mismatches | all Fetch exact |
|---:|---:|---:|---:|---:|---:|---:|:---:|
| 8192 | 14 | 7.88167e-06 | 0 | 1 | 0 | 1 | yes |
| 16384 | 14 | 0.0163578 | 0 | 3 | 2 | 2 | yes |
| 32768 | 14 | 5.65706e-05 | 0 | 33 | 16 | 22 | yes |

Every exact/plaintext Locate result matched the frozen oracle. Locate rejected accepted guard-crossing positions, deduplicated stable RecordIds, and applied deterministic ascending-RecordId First_K selection. Every selected frame reconstructed the exact stable ID and payload; every unselected physical window was zero.

Selector probes covered S sizes 0, 1, <K, =K, and >K. Every probe used one query ciphertext and one response ciphertext. Within each N, serialized query and response sizes were independent of |S|. S=empty decrypted to an all-zero slot vector.

The retained RFPS results contain 25 CKKS decision disagreements (1 at N=8192, 2 at N=16384, 22 at N=32768). These are permitted by the Phase-2 gate and are not hidden or merged with semantic RFPS acceptance. `04_correctness.csv` records N, |T|, L, m, threshold, exact score, decrypted score, and position for every event.

RFPS semantic false acceptance is not CKKS numerical error. Exact Fetch reconstruction only proves reconstruction of the IDs selected by Match and Locate; it does not prove semantic RFPS correctness.

## HE profiles and limits

All contexts use `HEStd_128_classic`. FPS uses the retained repaired tuples: FPS-13 `(N=8192, depth=2, b0=60, bs=50, FIXEDMANUAL, BV)`, FPS-14 `(N=16384, depth=2, b0=60, bs=40, FIXEDMANUAL, HYBRID)`, FPS-15 `(N=32768, depth=2, b0=60, bs=50, FIXEDMANUAL, HYBRID)`. RFPS retains depth 1, b0 60, bs 24. BGV-RNS retains t 65537, depth 1, and ell 128.

Actual T is used. No T is truncated. All three databases consume 50% of available CKKS transform slots and 100% of BGV record windows, so Fetch is the limiting stage.

## Frozen benchmark inputs

The subsequent formal benchmark should use exactly:

- `CBPIR-13`: N=8192, n=64, |T|=1629, L=2048, ell=128.
- `CBPIR-14`: N=16384, n=128, |T|=3201, L=4096, ell=128.
- `CBPIR-15`: N=32768, n=256, |T|=6544, L=8192, ell=128.

Do not substitute diagnostic Phase-2 timings for the formal seven-procedure measurements.
