# Published evidence extraction

## Primary source

The primary source is the local Choi et al. paper, `literature/3.search-and-retrieval/choi_2021_2109.07708v1.pdf`. Sections 4.2 and 7, Algorithms 1, 2, and 7, and Figures 3--5 were inspected directly.

All curated observations are in `01_paper_data.csv`. Textual values and integers printed in Figure 5 are kept separate from coordinates read from plotted figures.

## Exact or text-reported observations

- `fp=16`, `lambda=40`, and `eta=2` for BF-COIE.
- The authors measure from Algorithm 7 step 4 onward, after the server has encrypted match indicators.
- Databases contain 1,000--100,000 items; result sizes range from 8 to 128; items are 16-bit integers.
- `n=10,000,s=16` takes 16.7 seconds in total.
- Figure 5 reports 1,323 FHE ciphertexts, 32 PIR requests, and two FHE ciphertexts with batching.
- The prose gives approximately 103 KB per FHE ciphertext and 368.6 KB per SealPIR request.
- Computation uses one thread and no computation batching; the network is 1 Gbit/s.

## Figure extraction

The embedded raster for Figure 4 has a vertical scale of about 7.2 pixels per second. The stacked components were read at `s={8,16,32,64,128}`. One pixel is about 0.14 seconds, so these values are approximate. Server and client components calibrate Locate computation; the PIR-retrieval component calibrates Fetch; the network component is retained only for validation because CBPIR procedure timings exclude transport.

Figure 3's `n=10,000,s=16` point is anchored by the exact 16.7-second prose value. Marker centers for `n=20,000` through `100,000` were read with about two seconds of visual uncertainty. These points validate the broad linear trend but do not directly set the 12 target values.

## Internal consistency check

At `n=10,000,s=16`, the digitized Figure 4 stack sums to about 16.39 seconds, close to the 16.7-second text value. Excluding its network component gives about 15.56 seconds of computation. The structural fitted model gives about 15.27 seconds for Locate plus Fetch at that point. This is a useful sanity check, not calibration to force equality.

## Unresolved 1,323-ciphertext result

The accepted formula gives `ell=ceil(2s*sqrt(max(2s,s+32)))=222` for `s=16`. Plausible floor/ceil hierarchy conventions at `n=10,000` yield 1,998 or 2,220 Bloom-filter cells, plus possibly one count ciphertext. None gives 1,323. The discrepancy remains **UNRESOLVED PAPER/IMPLEMENTATION DETAIL**.

Communication therefore uses the published-formula count. Floor-versus-ceiling `ell` is the only target-cell rounding sensitivity. The displayed 1,323 value remains a disclosed limitation and is not transferred to the target workloads.
