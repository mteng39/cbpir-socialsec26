# CBPIR interactive benchmark CLI v1

## Status

Approved design. This document freezes the first user-facing benchmark CLI.
Implementation must remain additive and must not change frozen paper runners,
inputs, outputs, or evidence.

## Command

Extend the root `cbpir` binary with a `run` subcommand:

```bash
cargo run --release --features native --bin cbpir -- run [OPTIONS]
```

The existing smoke interface remains supported:

```bash
cargo run --release --features native --bin cbpir -- --quick
```

## Options

```text
Usage: cbpir run [OPTIONS]

Options:
  --dataset <DATASET>          Dataset [default: bgl]
  --num-records <N>            Records: 128, 256, 512, 1024
  --searchable-bytes <BYTES>   Searchable bytes per record
  --payload-bytes <BYTES>      Retrieved payload bytes per record
  --record-slots <SLOTS>       Fetch window: 64,128,224,256,384,512
  --capacity <K>               Retrieval capacity: 1,4,8,16
  --pattern-length <M>         Pattern bytes: 1,4,8,16
  --search-mode <MODE>         fps, rfps, both
  --warmups <COUNT>            Warmup executions [default: 1]
  --trials <COUNT>             Measured executions [default: 5]
  --output <PATH>              Optional JSON report
  --help                       Print help
```

Options without displayed defaults are required. V1 does not expose
`--num-clients`: CBPIR has no cross-client batching implementation.

## BGL workload generation

V1 accepts only `--dataset bgl` and reads the pinned `BGL_2k.log` fixture.
Generation is deterministic:

1. Parse eligible BGL records using the retained workload rules.
2. Select the first `N` records in source order.
3. Make each searchable field exactly `--searchable-bytes` long. Truncate a
   longer source field. Extend a shorter nonempty field by repeating its source
   bytes and truncate the repetition to the requested length.
4. Make each payload exactly `--payload-bytes` long using the same
   truncate-or-repeat rule.
5. Preserve record boundaries and stable record IDs.
6. Enumerate in-record patterns of the requested length and select support
   nearest the requested `K`; break ties by first record, first offset, then
   byte value, matching the retained fixture-selection rule.
7. Derive literal alignments and record IDs from the generated searchable
   fields. These values are the correctness oracle.

The complete generated workload, selected pattern, source checksum, and
generation parameters appear in the JSON report. Interactive workloads are
exploratory and never replace frozen paper evidence.

## Validation

Validate all cheap constraints before OpenFHE setup:

```text
num_records in {128,256,512,1024}
capacity in {1,4,8,16}
pattern_length in {1,4,8,16}
record_slots in {64,128,224,256,384,512}
searchable_bytes > 0
pattern_length <= searchable_bytes
payload_bytes > 0
payload_bytes <= 114
14 + payload_bytes <= record_slots
warmups >= 0
trials > 0
```

Then validate derived capacities:

- framed searchable text and transform length fit a supported CKKS ring;
- `num_records * record_slots` fits a supported BGV ring;
- the generated pattern lies entirely within a record;
- the selected candidate set is nonempty;
- the selected record count does not exceed public capacity `K`;
- all serialization and exact-reconstruction checks are available.

Every rejection identifies the supplied value, violated constraint, and
supported values. Invalid configurations fail before benchmark timing begins.

## Cryptographic parameter selection

Search and Fetch may use different ring dimensions.

### Search

1. Frame searchable records using the existing record-boundary convention.
2. Compute transform length `L`.
3. Select the smallest CKKS ring in `{8192,16384,32768,65536}` whose complex
   slot capacity is at least `L`.
4. Reject the configuration if no supported ring fits.

### Fetch

1. Compute `required_slots = num_records * record_slots`.
2. Select the smallest power-of-two BGV ring that fits, up to `131072`.
3. Use batching-compatible plaintext modulus `786433` and the existing
   128-bit security configuration.
4. Retain one packed encrypted Fetch query and one encrypted Fetch response.

Not every option cross-product is supported. Capacity failures are explicit;
the CLI never silently changes requested record count, field sizes, or `K`.

## Execution

- Run one Search mode at a time. `both` runs FPS then RFPS sequentially and
  releases the first mode's context before constructing the second.
- Use one client and one server execution context.
- Run requested warmups without recording them as measured samples.
- Run requested measured trials with the existing seven procedure boundaries.
- Measure serialization outside cryptographic stage timing boundaries.
- Retain deterministic seeds in the report.
- Write temporary native artifacts only below
  `target/reproduction/interactive/`.

## Output

Stdout always receives one JSON document. `--output` writes the identical JSON
bytes to the supplied path. The schema name is `cbpir-interactive-v1`.

```json
{
  "schema": "cbpir-interactive-v1",
  "configuration": {},
  "workload": {},
  "parameters": {},
  "setup": {},
  "measurements": {
    "raw": [],
    "summary": {}
  },
  "communication": {},
  "correctness": {}
}
```

For each mode, `raw` contains every measured procedure duration. `summary`
contains mean, median, standard deviation, and p95 for Search, Locate, Fetch,
and the per-run Online sum. It also reports separately:

- median of per-run Online sums;
- sum of independently reported Search, Locate, and Fetch medians, matching the
  paper's R1.3 convention;
- serialized Search and Fetch query/response bytes;
- candidate and selected counts;
- exact Fetch reconstruction;
- FPS literal-oracle agreement;
- RFPS threshold discrepancies.

Communication values are serialized message sizes per execution/configuration,
not timing-sample statistics. Repeated sizes and any variation are reported
explicitly.

## Code isolation

Add only these root modules for the new interface:

```text
src/cli.rs
src/experiment_cli.rs
src/workload_cli.rs
```

`src/bin/cbpir.rs` dispatches between the legacy smoke interface and `run`.
Shared protocol crates may gain additive public constructors only where the new
runner needs explicit parameter objects. Existing constructors remain valid.

Do not modify frozen experiment binaries, Python drivers, input JSON, raw data,
summaries, or verification records.

## README structure

Reorganize the root README in this order:

1. Introduction, paper, and slides.
2. Running: requirements, build, quick run, and interactive example.
3. Options: exact generated CLI help and supported constraints.
4. Record and Item Sizes: searchable text, payload, 14-byte frame header,
   fixed Fetch windows, and ring-capacity relationship.
5. Interpreting Measurements: Setup, Search, Locate, Fetch, Online,
   communication, aggregation, and correctness fields.
6. Reproducing the Paper: frozen Table 2, Search correctness, pattern length,
   scalability, and derived BF-COIE evidence.
7. Repository Structure.
8. Acknowledgements.
9. Citing.
10. License.

The README must distinguish interactive exploratory runs from frozen measured
paper evidence and derived BF-COIE estimates.

## Tests

- CLI parsing and help output.
- Every enumerated option constraint and derived-capacity failure.
- CKKS and BGV ring selection boundaries.
- Deterministic BGL workload generation and source checksum.
- Exact searchable and payload lengths.
- Stable IDs and preserved record boundaries.
- Pattern contained within one record.
- Deterministic nearest-`K` pattern selection.
- Correct bounded selection for every supported `K`.
- JSON schema and identical stdout/file output.
- Existing frozen-evidence validators.
- One minimal native end-to-end smoke configuration.

## Non-goals

- Cross-client batching or a `--num-clients` option.
- Arbitrary external datasets in v1.
- Replacing or regenerating paper evidence.
- Claiming that every accepted option combination has been evaluated in the
  paper.
- Changing the protocol, security claim, or cryptographic primitive.
