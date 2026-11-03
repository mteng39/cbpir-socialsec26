# CBPIR

Rust implementation and reproducibility package for **CBPIR: Content-Based
Private Information Retrieval over Structured Databases via Homomorphic
Encryption** (SocialSec 2026).

[Paper and citation](paper/README.md) ·
[Presentation](slides/cbpir-socialsec26-slides.pdf)

CBPIR searches structured records privately and retrieves at most `K` selected
payloads. Search uses CKKS FPS/RFPS, Locate maps alignments to unique record IDs
on the client, and Fetch uses one packed BGV query and one encrypted response.

## Running

### Requirements

Linux x86-64 with AVX, Rust 1.91.1, CMake, a C++ compiler, Git, and Python 3.
Allow at least 4 GiB RAM. Large profiles can require more memory.

### Build

```bash
git clone https://github.com/mteng39/cbpir-socialsec26.git
cd cbpir-socialsec26
OPENFHE_BUILD_JOBS=1 ./openfhe/bootstrap_openfhe.sh
export OPENFHE_INSTALL_DIR="$PWD/openfhe/.openfhe/openfhe-1.5.1/install"
export LD_LIBRARY_PATH="$OPENFHE_INSTALL_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export CARGO_BUILD_JOBS=1 OMP_NUM_THREADS=1 OMP_DYNAMIC=FALSE
cargo build --release --features native --locked
```

First native build takes several minutes; subsequent runs reuse Cargo artifacts.

### Quick run

```bash
cargo run --release --features native --locked --bin cbpir -- --quick
```

Success produces JSON with `bounded_fixture_gate_passed: true`.
Legacy `--match-backend ct_ct|server_text_ct_pt` remains available.

### Interactive run

```bash
cargo run --release --features native --locked --bin cbpir -- run \
  --num-records 128 --searchable-bytes 16 --payload-bytes 16 \
  --record-slots 64 --capacity 16 --pattern-length 4 \
  --search-mode both --warmups 1 --trials 5 \
  --output target/reproduction/interactive/example.json
```

Stdout contains JSON; `--output` writes identical bytes to a file. Generated
workloads and timings are exploratory measurements. Native serialization files
are temporary and stay below `target/reproduction/interactive/`.

## Options

```text
Usage: cbpir run [OPTIONS] --num-records <NUM_RECORDS> --searchable-bytes <SEARCHABLE_BYTES> --payload-bytes <PAYLOAD_BYTES> --record-slots <RECORD_SLOTS> --capacity <CAPACITY> --pattern-length <PATTERN_LENGTH> --search-mode <SEARCH_MODE>

Options:
      --dataset <DATASET>                    [default: bgl]
      --num-records <NUM_RECORDS>
      --searchable-bytes <SEARCHABLE_BYTES>
      --payload-bytes <PAYLOAD_BYTES>
      --record-slots <RECORD_SLOTS>
      --capacity <CAPACITY>
      --pattern-length <PATTERN_LENGTH>
      --search-mode <SEARCH_MODE>            [possible values: fps, rfps, both]
      --warmups <WARMUPS>                    [default: 1]
      --trials <TRIALS>                      [default: 5]
      --output <OUTPUT>
  -h, --help                                 Print help
```

Options without defaults are required. CBPIR has one client per execution;
there is no cross-client batching or `--num-clients` option.

## Record and Item Sizes

Searchable text and retrieved payload are separate byte fields. Interactive BGL
runs take the first eligible source records in deterministic order. Each field
is truncated or extended by repeating its original message bytes to its requested
length. Stable IDs and record boundaries are preserved. The query is an
in-record substring with record support nearest requested `K`.

Payloads are 1–114 bytes. The frame adds a 14-byte header, so
`14 + payload_bytes <= record_slots`. Pattern length must not exceed searchable
bytes. “Item size in bits” would conflate these two fields.

Search chooses the smallest supported CKKS ring with enough complex slots for
transform length `L`. Fetch chooses the smallest supported BGV ring fitting
`num_records * record_slots`, up to 131072, with plaintext modulus 786433.
Search and Fetch rings can differ. Excessive combinations fail before HE setup;
accepted dimensions do not guarantee a machine has sufficient memory.

## Interpreting Measurements

Reports use schema `cbpir-interactive-v1` and include the complete generated
workload, source checksum, selected pattern, HE parameters, and RFPS seed.

- `setup`: context generation, key generation, and database preparation.
- `measurements.raw`: each measured execution, procedure times in nanoseconds,
  candidate/selected counts, randomizer, serialized sizes, and correctness.
- Search = SearchQuery + SearchResponse.
- Locate includes decryption, inverse transform, boundary filtering,
  deduplication, and bounded selection.
- Fetch = FetchQuery + FetchResponse + Recover.
- Online excludes Setup and serialization. Reports retain both median of
  per-run Online sums and the R1.3 sum of independent stage medians.
- Summaries give mean, median, population standard deviation, and nearest-rank
  p95. Warmups are excluded.
- Candidate and selected counts have summaries in count units. Top-level
  correctness counts sum across measured trials; its flags require every trial
  to pass. Each raw row retains its own correctness results.
- Communication counts serialized Search and Fetch query/response bytes per
  execution. Repeated-size equality is checked; raw rows preserve variation.
- Correctness checks exact reconstruction separately from Search decisions.
  FPS must agree with the literal oracle. RFPS reports threshold disagreements,
  intrinsic cancellations, and threshold-only matches.

These are local computation measurements, not network latency. `both` runs FPS
then RFPS and releases contexts between modes.

## Reproducing the Paper

Retained evidence and interactive generated workloads have separate purposes:

| Evidence | Reproduction entry |
|---|---|
| Frozen Table 2 measurements | [Formal benchmark](evaluation/formal-benchmark/README.md) |
| Search decisions and error attribution | [Search correctness](evaluation/search-correctness/README.md) |
| R1.4 pattern-length extension | [Pattern length](evaluation/pattern-length-v1/README.md) |
| R2.10 512/1024-record extension | [Scalability](evaluation/scalability-v1/README.md) |
| Derived BF-COIE estimates and sensitivity | [BF-COIE estimator](evaluation/bf-coie-baseline/README.md) |
| Final aggregate ratios and R1.3 convention | [Final comparison](evaluation/final-paper/README.md) |

BF-COIE was not reimplemented or locally measured. Frozen evidence must not be
overwritten. See [REPRODUCE.md](REPRODUCE.md) for validation and campaign commands.

## Repository Structure

| Path | Purpose |
|---|---|
| `src/` | Primary crate, interactive CLI, protocol, benchmark binaries |
| `crates/ckks-search/` | CKKS FPS/RFPS Search |
| `crates/bgv-cpir/` | BGV packed retrieval |
| `crates/multi-fetch/` | Multi-record Fetch |
| `openfhe/` | Pinned dependency bootstrap |
| `scripts/` | Campaign drivers and validators |
| `data/` | Pinned BGL fixture and workloads |
| `evaluation/` | Retained raw results and summaries |
| `design/` | Approved interface specification |
| `tools/` | Historical diagnostics and studies |
| `paper/`, `slides/` | Publication information and presentation |

## Acknowledgements

Built with OpenFHE, OpenFHE-rs, and the Loghub BGL dataset. Upstream software
and data retain their respective licenses.

## Citing

```bibtex
@inproceedings{iasenovets2026cbpir,
  title = {CBPIR: Content-Based Private Information Retrieval over Structured Databases via Homomorphic Encryption},
  author = {Iasenovets, Artur and Tang, Fei},
  booktitle = {NSS-SocialSec 2026},
  year = {2026}
}
```

## License

Original software and project documentation are available under the
[MIT License](LICENSE).

This license does not cover:

- the Loghub input under `data/source/loghub/`, which retains its upstream
  license;
- the conference slides under `slides/`;
- the paper and any future manuscript copy under `paper/`; or
- third-party dependencies, which remain under their own licenses.
