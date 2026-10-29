# Pinned Loghub BGL sample

This directory retains the official 2,000-record BGL sample used by the
P0-B/P0-C realistic Match workload. It is not the full BGL corpus.

## Provenance

- Repository: <https://github.com/logpai/loghub>
- Pinned `master` commit:
  `dd61d0952749ee7963bde24220d1be5ede023033`
- Source path: `BGL/BGL_2k.log`
- Source URL:
  <https://raw.githubusercontent.com/logpai/loghub/dd61d0952749ee7963bde24220d1be5ede023033/BGL/BGL_2k.log>
- Source SHA-256:
  `2a819ea540909db682005c9cf948387a40729b5c2e9f19d430e29ce704825496`
- Source size: 317,150 bytes
- Logical records: 2,000
- Upstream license path: `LICENSE`
- License SHA-256:
  `9ffa6ae259833cdc6e7ed8a0219fec72ef6455fb457b3c44096cb1ac1f0696d4`
- License size: 553 bytes

The file contains 1,999 newline bytes. Its final record has no newline
terminator; line-oriented readers still produce 2,000 logical records.

## Terms and citation

The retained upstream `LICENSE` permits research or academic use subject to
repository attribution, citation of the Loghub paper where applicable, and
retention of the license notice in copies. This local note does not replace
those terms.

Upstream BGL documentation requests citation of:

- Adam J. Oliner and Jon Stearley, “What Supercomputers Say: A Study of Five
  System Logs,” DSN 2007.
- Jieming Zhu, Shilin He, Pinjia He, Jinyang Liu, and Michael R. Lyu,
  “Loghub: A Large Collection of System Log Datasets for AI-driven Log
  Analytics,” ISSRE 2023.

Authoritative upstream metadata:

- BGL README:
  <https://github.com/logpai/loghub/blob/dd61d0952749ee7963bde24220d1be5ede023033/BGL/README.md>
- Loghub license:
  <https://github.com/logpai/loghub/blob/dd61d0952749ee7963bde24220d1be5ede023033/LICENSE>

## Verification

```bash
sha256sum BGL_2k.log LICENSE
wc -c BGL_2k.log LICENSE
awk 'END {print NR}' BGL_2k.log
tail -c 1 BGL_2k.log | od -An -t u1
```

Expected last-byte decimal value is `51` (`3`), confirming no terminal
newline.
