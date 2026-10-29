# CBPIR-to-BF-COIE parameter mapping plan

## Different schemes remain different

| property | CBPIR BGV-RNS Fetch parameters | matched BF-COIE BFV configuration |
|---|---|---|
| library | OpenFHE 1.5.1 / OpenFHE-rs | Microsoft SEAL 3.6.6 plus historical SealPIR |
| scheme | BGV-RNS | BFV |
| ring dimension | 8,192; 16,384; 32,768 | same three polynomial modulus degrees, subject to runtime validation |
| security target | `HEStd_128_classic` | SEAL `sec_level_type::tc128` |
| plaintext modulus | 65,537 | proposed matched local choice 65,537; not an authors' parameter |
| depth | retained depth 1 | BF-COIE COIE uses additions only; SealPIR still needs its own validated operations/keys |
| fixed logical record | 64 slots, one byte per slot | one 64-byte PIR element |

## Frozen CBPIR target

| profile | `N` | `n` | public `K` | record slots | record bytes |
|---|---:|---:|---|---:|---:|
| CBPIR-13 | 8,192 | 128 | 1, 4, 8, 16 | 64 | 64 |
| CBPIR-14 | 16,384 | 256 | 1, 4, 8, 16 | 64 | 64 |
| CBPIR-15 | 32,768 | 512 | 1, 4, 8, 16 | 64 | 64 |

The 64-byte conclusion is source-traced, not inferred from `ell=64` alone:

1. P0-D `encode_frame` makes a 128-byte frame: 4-byte magic, 8-byte stable ID, 2-byte length, payload, zero suffix.
2. `inspect_fixed_frame` defines the meaningful prefix as `14+payload_length` and validates the suffix as zero.
3. `FormalFetchContext::prepare_database` allocates `record_slots` bytes, copies the frame prefix, and passes those exact vectors to P0-A.
4. The frozen runner sets `record_slots=64`; P0-A maps one unsigned byte to one packed BGV slot.
5. Recovery zero-extends the 64-byte record to 128 bytes before unchanged frame decoding.

Thus the controlled PIR-equivalent record is exactly 64 bytes. It contains the frame header, an 18--40 byte logical payload in the frozen databases, and zero padding to byte 64. Meaningful serialized prefixes are 32--54 bytes. Padding is 10--32 bytes per record. The BF-COIE baseline must retrieve the same 64-byte representation, not only the raw payload.

## Proposed local BFV policy

For each `N`:

1. Use SEAL 3.6.6 `EncryptionParameters(scheme_type::bfv)`.
2. Set `poly_modulus_degree=N`.
3. Set `coeff_modulus=CoeffModulus::BFVDefault(N, tc128)` and retain the generated primes verbatim.
4. Set the candidate `plain_modulus=65537`.
5. Construct `SEALContext` with `sec_level_type::tc128`; require `parameters_set`, `using_batching`, and reported security `tc128`.
6. Validate the exact historical SealPIR parameter generator and all required Galois/key-switch operations separately. Do not assume COIE feasibility proves PIR feasibility.

SEAL 3.6.6's HomomorphicEncryption.org 128-bit table permits maximum coefficient-modulus totals of 218, 438, and 881 bits for these three degrees. `BFVDefault` is the standard source-selected chain, not a hand-tuned chain.

## Plain modulus 65,537

65,537 is prime. It is congruent to 1 modulo `2N` for all three degrees:

- `65537 mod 16384 = 1` for `N=8192`;
- `65537 mod 32768 = 1` for `N=16384`;
- `65537 mod 65536 = 1` for `N=32768`.

This meets SEAL's BFV batching condition. It also exceeds the intended BF-COIE count values (`s<=16`; conservative per-cell total no larger than `eta*s<=32`) and byte values, so those values do not wrap.

This makes 65,537 a suitable **matched local plaintext-modulus choice** for the COIE side. It is not evidence that Choi et al. used 65,537. SealPIR computes packing from `floor(log2(t))`; compatibility, expansion ratio, response correctness, and security must be checked in the next dependency phase before the policy is frozen.

## Context-only feasibility probe

A minimal, untimed SEAL 3.6.6 probe constructed BFV contexts with `BFVDefault(N,tc128)` and exact `plain_modulus=65537`. It did not implement BF-COIE or SealPIR.

| `N` | parameters set | batching | security qualifier | coefficient primes | summed prime bits | slots |
|---:|---|---|---:|---:|---:|---:|
| 8,192 | yes | yes | 128 | 5 | 218 | 8,192 |
| 16,384 | yes | yes | 128 | 9 | 438 | 16,384 |
| 32,768 | yes | yes | 128 | 16 | 881 | 32,768 |

This answers context feasibility. It does not prove noise sufficiency for the historical SealPIR recursion, because that path has not yet been built or executed.

## Historical dependency anchors

- Microsoft SEAL 3.6.6: commit `d045f1beff96dff0fccc7fa0c5acb1493a65338c`.
- SealPIR paper-era candidate: commit `c7916baa05b43a5c96b8de094232a64f168515a2`.
- That SealPIR revision calls `CoeffModulus::BFVDefault(N)` and `PlainModulus::Batching(N, logt+1)`, verifies batching and key switching, and derives record packing from actual element bytes. The next phase must decide whether exact 65,537 can replace its generated same-bit-size prime without changing correctness.
