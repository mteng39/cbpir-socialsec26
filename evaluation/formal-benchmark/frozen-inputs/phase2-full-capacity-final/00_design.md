# Final Fixed-Window CBPIR Configuration Design

## Scope

Freeze one full-capacity fixed-window CBPIR database for each paired ring dimension `N in {8192,16384,32768}`. This is a configuration and correctness gate, not the formal seven-procedure benchmark.

## Retained frame semantics

`encode_frame()` and `decode_frame()` remain unchanged. Each record is first encoded into the existing 128-byte `P0D1` frame. The embedded two-byte payload length defines the meaningful prefix:

`required_slots = 14 + payload_length`.

The adapter validates `required_slots <= 128` and requires the remaining bytes to be zero. For a fixed candidate width `w`, the record fits exactly when `required_slots <= w`. The adapter packs the first `w` bytes when `w <= 128`; for `w > 128`, it copies the 128-byte frame and leaves the rest of the fixed window zero. Recovery copies the recovered window prefix into a zero-initialized 128-byte frame and calls the existing decoder.

Windows remain fixed and index-derived:

`J_i = [i*w, (i+1)*w)`.

No variable record span or variable-width encoder is introduced.

## Fixed Fetch adapter

Add a separate P0-D adapter for the final study. It uses the existing P0-A fixed-window database, selector, ciphertext-plaintext evaluation, decryption, and serialization APIs. Existing Phase-2 `NoDummyFetch` remains unchanged.

The adapter accepts no `K` and performs no truncation. Locate alone applies deterministic First_K. Empty selection is encrypted normally as one all-zero BGV selector. Every valid request emits one encrypted query and one encrypted response.

For widths that do not divide `N`, P0-A uses `floor(N/w)` complete windows and leaves the final tail slots zero. This removes an unnecessary divisibility guard; it does not alter any existing divisible-width profile.

## Workload and configuration selection

Reuse the frozen BGL file, SHA-256, ranking seed, stable source-line IDs, and nested-prefix rule. Audit the complete eligible pool through the real `encode_frame()` implementation.

For every `(N,w)` with `w in {64,128,224,256,384,512}`, scan actual nested prefixes and retain the largest prefix satisfying:

- every encoded record has `required_slots <= w`;
- `n*w <= N`;
- `L(T(n)) <= N/2` for actual guarded text `T(n)`.

For each `N`, select the largest feasible `n` under the smallest-valid-width rule. Regenerate the deterministic query corpus for those final databases.

## Verification

Use failing tests before implementation. Verify frame round trips for every eligible record and every fitting width, non-divisor-width unused tails, one query/response, size independence for realized `k=0..16`, First_K ownership in Locate, exact selected payloads, zero unselected windows/tail, FPS correctness, preserved RFPS classifications, pinned HE profiles, serial regressions, and byte-for-byte original Phase-2 evidence hashes.

