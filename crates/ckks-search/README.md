# CKKS private Search

Internal crate implementing Fourier Private Search (FPS) and Randomized Fourier
Private Search (RFPS) with OpenFHE CKKS. It provides:

- DFT-domain layout and transform helpers;
- FPS/RFPS query and server-response stages;
- staged response decryption and score classification;
- serialization and named parameter profiles;
- pinned workload parsing.

Search returns approximate alignment scores. Root `cbpir` package performs
record-boundary handling, stable-ID mapping, bounded selection, and Fetch.

```bash
cargo test --locked -p cbpir-search -- --test-threads=1
```

Tests require `OPENFHE_INSTALL_DIR`; see repository `README.md`.
