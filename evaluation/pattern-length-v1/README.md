# Pattern-length benchmark (R1.4)

This additive benchmark measures CBPIR at pattern lengths
`m = {1, 4, 8, 16}`. It does not replace the frozen `m=1` Table 2 evidence.

## Matrix

- Records: `n = {128, 256, 512}`
- Search: FPS and RFPS
- Retrieval capacity: `K=16`
- Repetitions: 3 warmups and 20 measured executions
- Total: 24 configurations and 480 measured executions
- OpenFHE: 1.5.1, one OpenMP thread

Patterns are deterministic, in-record BGL substrings. The `m=1` fixtures are
the frozen formal-benchmark patterns. Longer patterns use the retained
search-correctness selection rule: choose natural support nearest `K=16`, then
break ties by first record, offset, and byte value.

## Evidence

- `raw.csv`: every measured execution and all seven procedure boundaries.
- `summary.csv`: mean, median, standard deviation, and p95 by configuration.
- `fixtures.json`: exact patterns and source-workload hashes.
- `environment.txt`: build/runtime environment and peak process resources.
- `verification.txt`: matrix, communication, and correctness gates.
- `logs/`: exact native inputs, outputs, stderr, and process observations.

Search communication is constant across `m` within each fixed ring and Search
mode. All FPS decisions match the literal oracle. RFPS threshold discrepancies
are retained in the raw and summary data. Every selected payload is reconstructed
exactly.

Reproduction tooling is in `scripts/pattern-length/`.
