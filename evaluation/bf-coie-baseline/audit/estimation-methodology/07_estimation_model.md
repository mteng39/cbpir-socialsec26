# Symbolic latency and communication estimator

## Latency symbols

Use measured kernels rather than assuming isolated operations compose perfectly:

- `T_reduce(N,n)`: encrypted `n`-way Hamming-weight reduction.
- `T_count_dec(N)`: count ciphertext decryption and scalar extraction.
- `T_handoff`: in-memory count handoff processing; network transport is excluded from computation latency.
- `T_bf_accum(N,n,s,rule,seed)`: full encrypted BF hierarchy accumulation.
- `T_bf_batch_dec(N,c)`: decrypt and extract `c` BF cells.
- `T_bf_traverse(n,s,fp,eta,seed)`: plaintext Algorithm 2 traversal.
- `T_pir_batch_query(N,n,r,q)`, `T_pir_batch_answer(...)`, and `T_pir_batch_recover(...)`, where `r=64` bytes and `q=s+fp`.

Lower-level diagnostic symbols such as `tau_add(N)`, `tau_dec(N)`, `tau_ser(N)`, `tau_deser(N)`, `tau_hash`, and `tau_bfcheck` may explain kernels, but they are not the preferred estimator inputs.

## Candidate estimator

\[
\widehat T_{Count}=T_{reduce}(N,n)+T_{count\_dec}(N)+T_{handoff},
\]

\[
\widehat T_{Encode}=T_{bf\_accum}(N,n,s,rule,seed),
\]

\[
\widehat T_{Decode}=T_{bf\_batch\_dec}(N,c)+T_{bf\_traverse}(n,s,fp,eta,seed),
\]

\[
\widehat T_{Locate}=\widehat T_{Count}+\widehat T_{Encode}+\widehat T_{Decode},
\]

\[
\widehat T_{Fetch}=T_{pir\_batch\_query}+T_{pir\_batch\_answer}+T_{pir\_batch\_recover},
\]

\[
\widehat T_{PostMatch}=\widehat T_{Locate}+\widehat T_{Fetch}.
\]

Every resulting value is Class `E`. It is not measured end-to-end BF-COIE time.

## Why scalar addition is insufficient

BF accumulation has collision-dependent first assignments, ciphertext copies, allocator costs, hash costs, and a working set that changes with `N` and `c`. Count reduction has different cache and allocation behavior from one addition. SealPIR can reuse preprocessed database state across requests. Therefore direct kernels are mandatory. Isolated primitive timings remain diagnostics only.

## Communication model

Measure canonical serialized bytes of actual local objects.

Locate communication consists of:

1. encrypted count, server to client;
2. plaintext `s`, client to server;
3. unbatched encrypted BF hierarchy, server to client;
4. hash seeds/descriptions actually required by the local protocol.

Fetch communication consists of every query and response for `q=s+fp` SealPIR requests.

Do not use `(1+c)*ciphertext_bytes` until local object serialization is measured, because ciphertext compression and metadata can vary. The unresolved 1,323 count also prevents treating that expression as an authors-implementation fact.

## Table 2 mapping

The clean recommendation is to add a **Locate communication** column:

| Table field | future BF-COIE entry |
|---|---|
| Setup | `--` |
| Match latency | `--` |
| Locate latency | estimated Count + Encode + Decode (`E`) |
| Fetch latency | estimated SealPIR batch (`E`) |
| Online latency | `--` |
| Match communication | `--` |
| Locate communication | locally serialized Count + COIE messages (`M`, then summed) |
| Fetch communication | locally serialized SealPIR queries + responses (`M`, then summed) |
| Total post-Match communication | Locate + Fetch, explicitly labelled post-Match |

This is option **B**. It preserves semantics for CBPIR too and avoids hiding BF-COIE's large COIE transmission inside PIR Fetch. If the schema cannot change, the fallback is a footnoted post-Match aggregation, not an unqualified Fetch value.

