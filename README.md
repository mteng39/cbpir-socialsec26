# CBPIR

Rust implementation and reproducibility package for **CBPIR: Content-Based
Private Information Retrieval over Structured Databases via Homomorphic
Encryption** (SocialSec 2026).

[Paper and citation](paper/README.md) ·
[Presentation](slides/cbpir-socialsec26-slides.pdf)

## Running

### Requirements

Linux x86-64 with AVX, Rust 1.91.1, CMake, a C++ compiler, Git, and Python 3.
At least 4 GiB RAM.

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

### Quick run

```bash
cargo run --release --features native --locked --bin cbpir -- --quick
```

Success produces JSON with `bounded_fixture_gate_passed: true`.

### Interactive run

```bash
cargo run --release --features native --locked --bin cbpir -- run \
  --num-records 128 --searchable-bytes 16 --payload-bytes 16 \
  --record-slots 64 --capacity 16 --pattern-length 4 \
  --search-mode both --warmups 1 --trials 5 \
  --output target/reproduction/interactive/example.json
```

Results print as JSON; `--output` also saves them to a file.

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

Options without defaults are required.

## Record and Item Sizes

`--searchable-bytes` sets searchable text length; `--payload-bytes` sets retrieved
payload length. BGL messages are truncated or repeated to these lengths.
Queries are deterministic in-record substrings with support nearest `K`.

Payloads allow 1–114 bytes. Each record includes a 14-byte header:
4-byte format marker, 8-byte record ID, and 2-byte payload length.
`14 + payload_bytes <= record_slots`. Pattern length must fit searchable text.

Supported values: `n={128,256,512,1024}`, `K={1,4,8,16}`, `m={1,4,8,16}`,
and record slots `{64,128,224,256,384,512}`. Ring dimensions are selected
automatically; Fetch supports up to 131072 slots.

## Interpreting Measurements

Times are local computation in nanoseconds. Warmups are excluded.

- **Search:** SearchQuery + SearchResponse.
- **Locate:** score recovery, boundary filtering, deduplication, and selection.
- **Fetch:** FetchQuery + FetchResponse + Recover.
- **Online:** Search + Locate + Fetch; excludes Setup and serialization.
- **Communication:** serialized query/response bytes per execution.

JSON includes workload, parameters, raw trials, and mean/median/standard-deviation/
p95 summaries. Both Online statistics are retained: median of per-run sums and
sum of stage medians. Correctness reports exact Fetch reconstruction separately
from FPS/RFPS search errors. Top-level correctness counts sum across trials;
flags require all trials to pass.

## Reproducing the Paper

Rerun the paper's fixed workloads:

```bash
python3 scripts/benchmark/run.py --verify
```

Interactive runs generate different workloads and use BGV modulus 786433;
Table 2 uses 65537. Fresh timings vary; retained data reproduce published totals.

| Evidence | Reproduction entry |
|---|---|
| Frozen Table 2 measurements | [Formal benchmark](evaluation/formal-benchmark/README.md) |
| Search decisions and error attribution | [Search correctness](evaluation/search-correctness/README.md) |
| Pattern-length extension | [Pattern length](evaluation/pattern-length-v1/README.md) |
| 512/1024-record extension | [Scalability](evaluation/scalability-v1/README.md) |
| Derived BF-COIE estimates and sensitivity | [BF-COIE estimator](evaluation/bf-coie-baseline/README.md) |
| Final aggregate ratios | [Final comparison](evaluation/final-paper/README.md) |

BF-COIE values are derived estimates. See [REPRODUCE.md](REPRODUCE.md) for
validation and experiment commands.

## Acknowledgements

Search builds on Aurel Feer's [Privacy Preserving String Search using
Homomorphic Encryption](https://doi.org/10.3929/ethz-b-000666357).
Implemented using [OpenFHE](https://github.com/openfheorg/openfhe-development)
and [OpenFHE-rs](https://github.com/iasenovets/openfhe-rs), with the
[Loghub BGL dataset](https://github.com/logpai/loghub). Upstream software and
data retain their respective licenses.

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

This license does not cover the Loghub input, conference slides, paper, or
third-party dependencies, which retain their respective licenses.
