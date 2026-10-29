# Final-paper aggregate comparison

`recompute_final_comparison.py` applies the camera-ready R1.3 convention:
each displayed CBPIR Online value is the arithmetic sum of its Search, Locate,
and Fetch medians after each stage is rounded to three decimals. It then computes
the ratio of aggregate BF-COIE post-Match estimates to aggregate measured CBPIR
full-online values over all 24 CBPIR configurations.

Run from repository root:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 \
  evaluation/final-paper/test_recompute_final_comparison.py

PYTHONDONTWRITEBYTECODE=1 python3 \
  evaluation/final-paper/recompute_final_comparison.py
```

Expected central results:

- CBPIR latency aggregate: `1204.992 ms`.
- BF-COIE duplicated post-Match latency aggregate: `37976.0364937486336 ms`.
- Latency ratio: `31.515592...`, reported as `31.5x`.
- Communication ratio: `14.845991...`, reported as `14.8x`.

These are qualified, model-dependent comparisons, not end-to-end or
hardware-normalized speedups. See `../bf-coie-baseline/` for provenance,
sensitivity, and unresolved communication limitations.
