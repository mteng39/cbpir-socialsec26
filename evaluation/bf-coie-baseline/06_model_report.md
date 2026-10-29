# Latency and communication model

## Structural terms

For each power-of-two target cell:

\[
t=\log_2(n/(2s)),\qquad L_{BF}=t+1,
\]

\[
m=\max(2s,s+32),\qquad
\ell=\left\lceil 2s\sqrt{m}\right\rceil,\qquad
c=L_{BF}\ell.
\]

The ceiling is an explicit local integer convention. Count uses `n-1` additions. Encode has `2nL_BF` hash/accumulation attempts and no ciphertext-ciphertext multiplication. The Decode check bound is `2s+t*m`. PIR uses `s+16` requests.

## Locate boundary and Figure 4 mapping

Choi et al. measure from Algorithm 7 step 4, when the server holds encrypted match indicators, until the client has recovered records. Within Figure 4:

- `Server` contains encrypted Count and server Encode;
- `Client` contains the count decryption plus BF-COIE output decryption and Decode;
- `Retrieval` is SealPIR;
- `Communication` is network time and is excluded from computation estimates.

The server fit uses Encode accumulation attempts as a proxy for the combined Count-plus-Encode server block because the paper does not expose Count separately. Count is therefore absorbed by the measured server calibration and is not added again.

## Locate central model

The selected central model uses the floor hierarchy interpretation for the non-power-of-two paper calibration workload and through-origin component fits:

\[
T_{Locate}(n,s)=
\alpha_{srv}^{floor} \cdot 2nL_{BF}
+\alpha_{cli}^{floor}\cdot c,
\]

where

\[
\alpha_{srv}^{floor}=7.2222222\times 10^{-5}\;\text{s/attempt},
\]

\[
\alpha_{cli}^{floor}=1.9754069\times 10^{-4}\;\text{s/ciphertext}.
\]

Calibration uses Figure 4 at `n=10000` and `s={8,16,32,64,128}`. Combined server-plus-client RMSE is `0.3944 s`. Residuals are `{-0.2152,-0.2164,-0.4131,-0.3852,+0.6048} s`.

The ceil-hierarchy through-origin model has a worse combined RMSE of `0.5545 s` and provides hierarchy sensitivity. Intercept fits have lower in-range RMSE (`0.2624 s` floor and `0.2736 s` ceil), but their server intercepts are negative (`-0.9167 s` and `-2.4722 s`). Clamping those intercept models produces unstable and sometimes non-monotone downward extrapolation. They remain diagnostic only. No model averaging is used.

Every target `n` is below the paper's measured `n>=1000` range, so all Locate values are `F+X`. The sensitivity interval spans only floor- versus ceil-hierarchy through-origin calibrations.

## Fetch central model

Let `q=s+16` be PIR request count. Figure 4 Retrieval is fit with:

\[
T_{Fetch}^{int}(q)=0.0820685857q-0.717032557\;\text{s},
\]

with RMSE `0.0217 s`, and the sensitivity alternative:

\[
T_{Fetch}^{origin}(q)=0.0744916953q\;\text{s},
\]

with RMSE `0.3978 s`.

For `K=1,4`, the intercept fit is central. It nearly reproduces the lowest measured point at `K=8` (`1.2526 s` predicted versus `1.2500 s` digitized), while the origin fit predicts `1.7878 s`. The negative intercept is not interpreted as negative setup cost. It is an empirical local extrapolator across the short step from the measured `q=24` to `q=20` and `q=17`. The origin fit is the upper sensitivity model.

For `K=8,16`, central values remain the digitized Figure 4 Retrieval anchors: `1.2500 s` and `1.9444 s`. The two fits provide sensitivity. No midpoint is used.

## 64-byte item normalization

Historical SealPIR source recommends `N=4096`, `logt=20`, and recursion depth `d=2`. Under this tuple, the paper workload and target `n<=256` use geometry `[2,2]`; target `n=512` uses `[3,2]`. With expansion ratio `R=12`, ciphertext-plaintext products change from `4+2R=28` to `6+2R=30`, giving `15/14`.

The authors do not publish their actual SealPIR tuple. Therefore `15/14` is retained as the best-supported central `n=512` factor, not as a recovered implementation fact. Parameter sensitivity is `1.0--1.5`; details are in `03_item_size_normalization.md`. All Fetch values are `D+F+X` because database size and 64-byte item geometry are outside the directly measured paper workload.

## Communication model

Using Choi et al.'s approximate object sizes:

\[
C_{Locate}=(1+c)\cdot 103{,}000\text{ bytes},
\]

\[
C_{Fetch}=(s+16)\cdot 368{,}600\text{ bytes}.
\]

The extra one in Locate is the encrypted Count ciphertext. MiB uses `2^20` bytes. Locate sensitivity is floor-versus-ceiling `ell` only. Figure 5's unresolved 1,323-ciphertext value is a disclosed limitation and is not a correction factor.

Network transfer time is not added to computation latency. Setup, Search, and Online remain unavailable.
