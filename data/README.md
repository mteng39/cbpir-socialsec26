# Data and workloads

`workloads/` contains compact JSON copies of inputs used by the formal performance
and Search-correctness binaries. Each input records selected BGL-derived records,
stable IDs, searchable bytes, payload bytes, record spans, parameters, and query
definitions.

`manifests/workload_manifest.csv` identifies each frozen input.
`manifests/18_batch_input_hashes.csv` records correctness-batch input hashes.

The pinned upstream `BGL_2k.log` and its license are retained under
`data/source/loghub/`. The complete search workload fixture is under
`data/workloads/search_workloads_v1.json`.

Recorded upstream provenance:

- Loghub commit: `dd61d0952749ee7963bde24220d1be5ede023033`
- `BGL_2k.log` SHA-256:
  `2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496`
- Selection seed: `p0d-bgl-k1-20260911-v1`
