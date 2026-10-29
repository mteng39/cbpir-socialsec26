# Functional-stage mapping: CBPIR and BF-COIE

## Scope and source discipline

This audit compares functional boundaries, not cryptographic operations. CBPIR terminology and procedures come from the current manuscript. Choi et al.'s native terminology comes from their generic `Setup`, `Query`, `Match`, and `Fetch` framework and Algorithm 7. Any use of `Locate` for BF-COIE is therefore a comparison-layer decomposition; Choi et al. do not name a BF-COIE phase `Locate`.

Three evidence labels are used below:

- **CBPIR-defined:** stated by the current manuscript.
- **Choi-defined:** stated by Choi et al.
- **Comparison-layer:** a decomposition introduced only to align functional start and end points.

## Native protocol boundaries

### CBPIR

The manuscript defines three online phases. `Search` runs `SearchQuery` and `SearchResponse`, starting with the private pattern and ending with an encrypted search-score response. `Locate` is client-local: it decrypts that response, recovers scores, accepts alignments, maps them to stable record identifiers, and applies `First_K` to obtain `S(P)`. `Fetch` runs `FetchQuery`, `FetchResponse`, and `Recover`, starting with `S(P)` and ending with the selected payloads.

### Choi et al.

Choi et al. define `Query` as sending an encrypted query and `Match` as homomorphically evaluating it against encrypted records to produce the encrypted indicator vector `[[b]]`. For the COIE construction, their native `Fetch` is Algorithm 7 steps 4--8. It starts with `[[b]]` at the server, recovers a candidate index set through Count and COIE Encode/Decode, pads the PIR workload, and retrieves records through PIR.

## Side-by-side stage map

| CBPIR stage | CBPIR input | CBPIR output | Choi native stage(s) | Choi input | Choi output | functional equivalence | caveat |
|---|---|---|---|---|---|---|---|
| `Setup` | plaintext database `D` already available to the protocol | query-independent plaintext search and fetch views | `Setup` | client plaintext database, FHE parameters/keys | encrypted database and evaluation state uploaded to server | **NOT FUNCTIONALLY EQUIVALENT** | Both are query-independent, but CBPIR builds server plaintext views while Choi encrypts and uploads the database. Key generation and upload also fall outside CBPIR's measured Setup boundary. |
| `Search` | private pattern `P`, public parameters, server search view | encrypted alignment-score representation `ct_r^search`, delivered to client | `Query` + `Match` | encrypted query and encrypted database | encrypted per-record match-indicator vector `[[b]]`, retained by server | **CLOSE FUNCTIONAL MATCH** | Both transform a private request into protected match evidence. CBPIR returns approximate alignment scores that still require thresholding; Choi produces encrypted record-level bits. State ownership at the boundary also differs. |
| `Locate` | encrypted score response | accepted alignments, candidates, and bounded stable-ID set `S(P)` | native `Fetch`, steps 4--7: Count, COIE Encode, decryption/Decode, abort/padding | encrypted indicator vector `[[b]]` | decoded candidate index set `S'`, then a PIR-ready list padded to `s+f_p` | **CLOSE FUNCTIONAL MATCH** | Both map protected match evidence to retrieval indices. CBPIR is client-local and returns at most `K` selected IDs; BF-COIE is interactive and may return a relaxed superset containing false positives plus dummy padding. `Locate` is not Choi-native terminology. |
| `Fetch` | selected stable record IDs `S(P)` | recovered selected payloads | native `Fetch`, step 8: PIR | decoded/padded index list | records returned by the PIR executions | **CLOSE FUNCTIONAL MATCH** | Both privately retrieve records from indices. CBPIR uses one packed query and response; BF-COIE runs `s+f_p` PIR requests and retrieves true, false-positive, and dummy items. |
| `Locate+Fetch` | protected search result | recovered logical records | complete native `Fetch`, steps 4--8 | encrypted indicator vector `[[b]]` | candidate records after COIE recovery and PIR | **CLOSE FUNCTIONAL MATCH** | Start and end roles align most closely, but CBPIR returns a bounded exact selected set whereas BF-COIE provides relaxed-superset correctness and padded retrieval. |

## Procedure-level map

There is no defensible one-to-one procedure map throughout because Choi et al. combine operations and communication differently.

| CBPIR procedure | Closest Choi operation | mapping | caveat |
|---|---|---|---|
| `Setup` | Algorithm 7 step 1 / generic `Setup` | not equivalent | Choi includes client encryption and encrypted-database upload; CBPIR derives reusable plaintext views from `D`. |
| `SearchQuery` | Algorithm 7 step 2 / `Query` | close | Both create and send an encrypted private query, but query encodings and HE schemes differ. |
| `SearchResponse` | Algorithm 7 step 3 / `Match` | partial | Both perform server-side homomorphic search evaluation. CBPIR sends an encrypted score response to the client; Choi retains the encrypted bit vector at the server for the next step. |
| `Locate` | Algorithm 7 steps 4--7 | close functional map, not a native procedure match | Choi's work is interactive and includes Count plus COIE Encode/Decode; CBPIR Locate is wholly client-local. |
| `FetchQuery` | client PIR query generation in step 8 | partial | Choi does not expose it as a separate secure-search procedure. Padding is specified at the end of step 7 and determines `s+f_p` queries. |
| `FetchResponse` | server PIR answer generation in step 8 | close | Both are server-side private-retrieval evaluations, but BF-COIE invokes SealPIR repeatedly. |
| `Recover` | client PIR reconstruction in step 8 | close | Both reconstruct returned records. BF-COIE's relaxed result may include records at false-positive indices; dummy indices are known padding rather than true results. |

## Verdicts on proposed mappings

### `Search` ↔ `Query+Match`: **CLOSE FUNCTIONAL MATCH**

The proposed mapping is sound at the phase level. Both begin with the client's private search request and end with a protected representation of matches. It is not exact: CBPIR produces encrypted alignment scores and transfers them to the client, whereas Choi's Match produces encrypted per-record bits that remain at the server. Consequently, CBPIR thresholding belongs to Locate, while Choi's matching predicate has already produced bits.

### `Locate` ↔ COIE index recovery: **CLOSE FUNCTIONAL MATCH**

Both stages convert protected match evidence into indices usable for retrieval. The algorithms and interaction differ materially: CBPIR decrypts locally and applies boundary filtering, stable-ID mapping, sorting, and `First_K`; BF-COIE performs an encrypted count exchange, server Encode, encrypted-encoding transfer, client Decode, an abort test, and padding. The mapping is faithful only when labeled as a comparison-layer decomposition of Choi's native Fetch.

### `Fetch` ↔ PIR retrieval: **CLOSE FUNCTIONAL MATCH**

Both start from record indices and end with recovered logical records. The private-retrieval mechanisms are not equivalent: CBPIR packs up to `K` selections into one query/response, while BF-COIE retains `s+f_p` PIR requests. BF-COIE's false-positive and dummy retrievals are protocol overhead, not additional true results.

### `Locate+Fetch` ↔ Choi measured `Fetch`: **CLOSE FUNCTIONAL MATCH**

This is the strongest aggregate alignment. Both start with a protected match result and end after record recovery. The match is not exact because CBPIR returns a bounded selected set and BF-COIE implements relaxed-superset correctness with padded PIR requests.

## What Choi et al.'s “Fetch time” contains

Section 7.1 explicitly measures from the point where the server holds `[[b_1]],...,[[b_n]]` to the point where the client recovers the records, namely Algorithm 7 steps 4 and later. It therefore includes:

1. server computation of encrypted `[[s]]` and its transfer to the client;
2. client decryption of `s` and return of plaintext `s`;
3. server BF-COIE Encode and transfer of the encrypted encoding;
4. client decryption and BF-COIE Decode, including abort/padding logic;
5. the PIR retrievals for the padded `s+f_p` index list.

It excludes Query and Match, including encrypted-query creation/transmission and production of `[[b]]`. Figure 4 further separates this native Fetch time into Server, Client, network communication, and Retrieval components. For the comparison table, Server+Client index-recovery computation maps to `Locate`, Retrieval maps to the narrower PIR `Fetch`, and communication is reported in separate communication columns. This decomposition does not change Choi et al.'s source-native name for the full steps 4--8 procedure.

Section 7.2 gives an end-to-end speedup estimate based on an assumed relation between LEAF and BF-COIE Match costs; it does not report a workload-compatible BF-COIE Query+Match measurement. Section 7.3 reports the native Fetch communication objects: the post-Match encrypted count/COIE material and the PIR requests. The encrypted Query and server-held Match vector lie outside that communication accounting.

## Setup verdict

CBPIR Setup and Choi Setup share only the broad property of being query-independent. CBPIR Setup constructs reusable plaintext search and fetch views at the server from the protocol database. Choi Setup encrypts and uploads every database record and provides evaluation material; the paper describes this construction as setup-free because it avoids a specialized encrypted search index, not because encryption/upload costs vanish.

These boundaries are not sufficiently aligned for a numerical Setup comparison. A table may retain a `Setup` column for CBPIR, but the BF-COIE Setup cell should remain `--` unless a separately defined and measured common preprocessing boundary is introduced.

## Aggregate-label decision

| candidate schema | boundary quality | decision |
|---|---|---|
| `Search | Locate | Fetch | Online` | First three functional columns are usable, but BF-COIE Online lacks Query+Match evidence. | Reject as the only aggregate schema. `Online` may remain for CBPIR-only reporting, with BF-COIE `--`. |
| `Search | Locate | Fetch | Locate+Fetch` | Gives the same functional post-match-result-to-record-recovery aggregate for both constructions. | **Recommended.** |
| `Search | Locate | Fetch | Retrieval` | `Retrieval` is ambiguous because the manuscript already uses retrieval language for the narrower CBPIR Fetch phase. | Reject. |
| `Query/Match | Index Recovery | PIR Retrieval` | Source-descriptive, but abandons current CBPIR terminology and still requires comparison-layer naming. | Reject for the main table; useful only as an explanatory diagram. |

The recommended aggregate is **`Locate+Fetch`**. `Post-Search` privileges CBPIR terminology; `Post-Match` privileges Choi terminology; `Retrieval` can be mistaken for the narrower PIR stage. `Locate+Fetch` states its arithmetic and boundary directly.

Full BF-COIE `Online` cannot be populated from current evidence. The missing stage is the workload-compatible computation and communication for Choi's Query+Match, from encrypted-query construction/transmission through production of `[[b]]`. Choi et al.'s reported Fetch values begin only after this stage.

The six aggregate questions therefore resolve as follows:

1. A common `Search` column is defensible because the functional role aligns, but BF-COIE remains `--` because Choi et al. do not report a compatible Query+Match value.
2. `Locate+Fetch` is the cleanest aggregate supported for both constructions.
3. `Post-Search` is source-biased toward CBPIR and is not recommended.
4. `Post-Match` is source-biased toward Choi et al. and is not recommended.
5. `Retrieval` is neutral in ordinary language but ambiguous with the narrower PIR/CBPIR Fetch stage; `Locate+Fetch` is preferable.
6. BF-COIE full `Online` cannot be populated because Query+Match computation and communication are missing.

## Communication map

| comparison column | CBPIR communication | BF-COIE communication | verdict |
|---|---|---|---|
| `Search` | encrypted Search query plus encrypted Search response | encrypted Query sent client-to-server; Match is server-local and leaves `[[b]]` at the server | functionally aligned, but BF-COIE value remains `--` because no workload-compatible measurement is available |
| `Locate` | `0` | encrypted count server-to-client; plaintext `s` client-to-server; encrypted COIE encoding server-to-client; any required hash descriptions | same functional index-recovery boundary, different interaction |
| `Fetch` | encrypted Fetch query plus encrypted Fetch response | all `s+f_p` PIR queries and responses | same functional record-retrieval boundary |
| `Locate+Fetch` | sum of Locate and Fetch communication; numerically equal to Fetch because Locate is zero | Count/COIE exchange plus padded PIR traffic | clean common post-match-result aggregate |

CBPIR Locate communication is exactly zero under the manuscript boundary because decryption, inverse DFT, score extraction, thresholding, record-boundary filtering, stable-ID mapping, deduplication, sorting, and `First_K` all execute locally at the data requester. BF-COIE index recovery is nonzero because Algorithm 7 steps 4--6 require an encrypted-count message, a plaintext count return, and transmission of the encrypted COIE encoding. BF-COIE Decode and padding are client-local; PIR communication starts only in step 8. Choi et al. do not specify whether Bloom-filter hash descriptions are transmitted or derived from shared public randomness, so any such bytes must remain an explicit accounting qualification.

The computation and communication tables should therefore use the same functional columns: `Search | Locate | Fetch | Locate+Fetch`. This avoids silently placing BF-COIE's COIE transmission in the narrower Fetch/PIR column.

## Workload axis

The primary comparison axis should be **`n`**, not HE ring dimension `N`. The common workload is defined by database cardinality `n={128,256,512}`, 64-byte logical records, and public capacity `K={1,4,8,16}`, with BF-COIE `s=K`. Choi et al.'s historical BFV ring dimension is unavailable and is not matched to CBPIR's `N`. CBPIR's associated `N={2^13,2^14,2^15}` remains available in the separate configuration table and should not be presented as a BF-COIE parameter.

The workload columns should be `n | K | k | Protocol`. For BF-COIE, `k=s=K` denotes true matching records; false-positive and dummy PIR requests do not increase `k`.

## Cells that must remain unavailable

For BF-COIE, retain `--` for:

- `Setup`, because the preprocessing boundary is not functionally aligned or measured comparably;
- `Search`, because Choi et al. do not report workload-compatible Query+Match cost;
- Search communication, for the same reason;
- full `Online`, if that column is retained, because Query+Match is missing;
- any full-online communication total that would require the missing Query communication size under the matched workload.

The comparison-layer `Locate`, `Fetch`, and `Locate+Fetch` estimates may be shown with their existing provenance and limitations. No numerical value is changed by this audit.

## Draft manuscript paragraph

BF-COIE is a complete secure-search construction with native Query, Match, and Fetch procedures, but Choi et al. report performance only after Match has produced the encrypted indicator vector. We map CBPIR Search to Choi's Query+Match at the functional level; because no workload-compatible Query+Match measurement is available for BF-COIE, its Search entry remains unavailable. Choi et al. use Fetch for all subsequent work, whereas our comparison decomposes their Algorithm 7 steps 4--7 as Locate and step 8 as Fetch. This comparison-layer split aligns index recovery and private record retrieval without implying that Choi et al. define a native Locate phase. CBPIR Locate communication is zero because it runs entirely at the client, while BF-COIE Locate communication includes the count exchange and encrypted COIE transmission. The common measured or estimated aggregate is therefore Locate+Fetch, which begins with a protected match result and ends with recovered logical records.

## Recommended table-header skeleton

```text
Workload                         Computation (ms)                                  Communication (MiB)                         Correctness
n | K | k | Protocol | Setup | Search | Locate | Fetch | Locate+Fetch | Search | Locate | Fetch | Locate+Fetch | Search | Retrieval
```

Here `n` is the common database-cardinality axis. `Setup`, `Search`, and `Locate+Fetch` are computation columns; the repeated stage names after the second group divider are communication columns. `Online` should not be the common aggregate because BF-COIE Query+Match is absent from the available evaluation evidence.
