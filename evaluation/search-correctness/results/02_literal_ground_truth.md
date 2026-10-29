# Literal Record-Level Ground Truth

Enumeration scans only windows wholly inside each frozen record. Multiple matches in one record map to one stable record ID.

| Profile | N | n | Pattern | Alignments | Record IDs | Records |
|---|---:|---:|---|---:|---|---:|
| CBPIR-13 | 8192 | 128 | `0x66` (`f`) | 16 | `[11, 293, 300, 306, 384, 386, 450, 1037, 1056, 1107, 1113, 1117, 1147, 1336, 1622, 1660]` | 16 |
| CBPIR-14 | 16384 | 256 | `0x2c` (`,`) | 31 | `[11, 291, 384, 385, 386, 871, 1151, 1223, 1237, 1329, 1459, 1467, 1499, 1528, 1786, 1944]` | 16 |
| CBPIR-15 | 32768 | 512 | `0x3a` (`:`) | 17 | `[177, 180, 194, 195, 197, 199, 203, 206, 217, 221, 238, 252, 284, 285, 306, 450, 1991]` | 17 |
