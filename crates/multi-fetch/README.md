# Bounded multi-record Fetch

Internal crate extending packed BGV retrieval to a bounded set of records. It
provides:

- stable deduplication and truncation to public capacity `K`;
- dummy-window planning for fixed-capacity layouts;
- one packed encrypted Fetch query and one encrypted response;
- exact ordered payload reconstruction;
- experimental sharded layouts kept separate from canonical one-ring Fetch.

Root `cbpir` package connects this Fetch stage to Search and client-local Locate.

```bash
cargo test --locked -p cbpir-fetch -- --test-threads=1
```

Tests require `OPENFHE_INSTALL_DIR`; see repository `README.md`.
