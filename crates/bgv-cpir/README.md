# BGV CPIR core

Internal crate for exact packed record retrieval. It provides:

- fixed-width record-window packing;
- one-hot and multi-window selectors;
- OpenFHE BGV query/response execution;
- exact binary serialization and record reconstruction;
- named parameter profiles used by CBPIR experiments.

This crate answers index-based retrieval only. Root `cbpir` package supplies
content Search, client-local Locate, and full protocol composition.

```bash
cargo test --locked -p cbpir-bgv
cargo test --locked -p cbpir-bgv --features openfhe-backend -- --test-threads=1
```

Native tests require `OPENFHE_INSTALL_DIR`; see repository `README.md`.
