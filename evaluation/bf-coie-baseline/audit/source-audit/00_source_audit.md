# BF-COIE Source-Code Recovery Audit

Audit date: 2026-09-25 UTC

## Decision

No authentic author-maintained BF-COIE implementation or ACM artifact was
recovered.

The paper, its LaTeX source, and its named dependencies are public. None of
those sources contains the BF-COIE experiment code, the Algorithm 7 driver, or
the paper's PySEAL-to-SealPIR integration. Any implementation produced for this
project must therefore be labelled **BF-COIE reimplementation**. It must not be
described as the authors' original implementation or as a recovery of their
exact HE parameters.

This audit is the required source-recovery gate. No BF-COIE implementation was
written before reaching this decision.

## Paper and publication artifacts

| Candidate | Owner | Last known revision | BF-COIE code | SealPIR integration | Build status | License visible | Decision |
|---|---|---:|---|---|---|---|---|
| [ACM DOI record](https://doi.org/10.1145/3460120.3484792) | ACM / paper authors | Published 2021-11-13 | No code located | Paper prose only | Not applicable | ACM publication terms | Reject as source artifact. Crossref exposes article/PDF links and no related artifact or supplement relation. Direct ACM page access returned HTTP 403 during this audit, so absence is based on discoverable metadata plus all other searches, not on a claim that ACM can never host an unindexed file. |
| [arXiv 2109.07708v1](https://arxiv.org/abs/2109.07708) | Paper authors; submitted by Linsheng Liu | v1, 2021-09-16 | No executable code | Paper prose only | LaTeX source available; no program to build | arXiv distribution license shown on record | Reject as implementation. The page offers PDF, HTML, and TeX source, but no attached code repository. |
| [IACR ePrint 2021/1199](https://eprint.iacr.org/2021/1199) | Paper authors; contact Arkady Yerukhimovich | 2021-09-17 | No executable code | Paper prose only | PDF only | CC BY | Reject as implementation. Metadata lists only the PDF and no auxiliary artifact. |
| Local paper `literature/3.search-and-retrieval/choi_2021_2109.07708v1.pdf` | Paper authors | arXiv v1, 2021-09-16 | No executable code or source URL | Names PySEAL, SEAL 3.6, and SealPIR | Not applicable | Same work as arXiv v1 | Accept as protocol/evaluation authority only. |
| [Linsheng Liu thesis site](https://linshengliu.com/) and [`lls1129/lls1129.github.io`](https://github.com/lls1129/lls1129.github.io) | Linsheng Liu | `be565a04c879fbac3fe7e1ad70b92fb35d953b0c`, 2025-12-31 | LaTeX/HTML restatement of Algorithms 1, 2, and 7; no executable implementation | Prose citation only | Static site builds; no BF-COIE program | No repository license visible | Accept as corroborating author material; reject as implementation. |
| [Seung Geol Choi publication page](https://www.usna.edu/Users/cs/choi/pub.html) and [`choiusna/choiusna.github.io`](https://github.com/choiusna/choiusna.github.io) | Seung Geol Choi | `311c84dc82b4d0e0e28e35d944277294e1297446`, 2021-09-09 | None | None | Static site only | No repository license visible | Reject. Publication page links the ePrint, not code. |
| [Arkady Yerukhimovich publication page](https://www2.seas.gwu.edu/~arkady/publications/) and [`arkady5/arkady5.github.io`](https://github.com/arkady5/arkady5.github.io) | Arkady Yerukhimovich | `6763a5063e894560b507f388f9fbc1fa2a0e46a4`, 2021-03-26 | None | None | Static site only | No repository license visible | Reject. Publication page links the ePrint, not code. |
| [Dana Dachman-Soled publication page](https://user.eng.umd.edu/~danadach/) | Dana Dachman-Soled | Live page checked 2026-09-25 | None | None | Not applicable | Not shown | Reject. Entry links the ePrint only. |

## Plausible code repositories and dependencies

| Candidate | Owner | Last known revision | BF-COIE code | SealPIR integration | Build status | License visible | Decision |
|---|---|---:|---|---|---|---|---|
| [Microsoft SealPIR](https://github.com/microsoft/SealPIR) | Microsoft Research / SealPIR authors | Current `cf55f10be7bffba4e78d5ba32764284ac828d6cd`; paper-era candidate `c7916baa05b43a5c96b8de094232a64f168515a2`, 2021-08-26 | No | Yes; this is the PIR dependency itself | Paper-era commit declares SEAL 3.6.6 and predates CCS publication | MIT | Accept as authentic SealPIR dependency, not as BF-COIE source. |
| [Microsoft SEAL](https://github.com/microsoft/SEAL) | Microsoft | SEAL 3.6.6 tag `d045f1beff96dff0fccc7fa0c5acb1493a65338c` | No | Required by paper-era SealPIR | Tagged source available | MIT for the relevant modern SEAL release | Accept as authentic HE dependency candidate, not proof of the authors' exact coefficient moduli or wrapper configuration. |
| [Lab41/PySEAL](https://github.com/Lab41/PySEAL) | Lab41; cited PySEAL authors include Alexander J. Titus et al. | `6be33dae293a26a1f7f64fb762902b3290d6e6ec`, 2019-07-10; archived 2020-08-18 | No | No BF-COIE/SealPIR glue | Repository documents SEAL 2.3, whereas the paper says SEAL 3.6 | Microsoft Research non-commercial license in `LICENSE.txt` | Reject as an exact runnable paper stack. Accept only as provenance for the cited Python wrapper lineage. The version mismatch shows that the public repository alone cannot reproduce the authors' stated setup. |
| Public forks of `Lab41/PySEAL` | 80 public forks enumerated by GitHub API on 2026-09-25 | Various | No author-owned or BF-COIE-labelled fork found | None found | Not tested | Varies | Reject. No fork owner matched the five paper authors and no repository metadata identified BF-COIE. |
| [`gogo9th/fhe-papers`](https://github.com/gogo9th/fhe-papers) | Third party | Live index checked 2026-09-25 | No; bibliography/index only | No | Not applicable | Repository-specific | Reject. Unrelated third-party paper list. |

## Search coverage

The following identifiers and strings were searched on the public web,
GitHub repository and commit search, Sourcegraph's public index, GitLab,
Bitbucket, Zenodo, Figshare, arXiv, IACR ePrint, ACM/Crossref metadata, author
publication pages, and known author GitHub accounts:

- exact paper title;
- DOI `10.1145/3460120.3484792`;
- arXiv `2109.07708`;
- ePrint `2021/1199`;
- `BF-COIE`, `BFCOIE`, `COIE.Encode`, and `COIE.Decode`;
- combinations of `COIE`, `PySEAL`, and `SealPIR`;
- all five author names and public institutional email addresses.

Known public author accounts inspected:

- [`choiusna`](https://github.com/choiusna);
- [`lls1129`](https://github.com/lls1129);
- [`arkady5`](https://github.com/arkady5).

GitHub repository search returned zero repositories for the exact title,
`BF-COIE`, `BFCOIE`, the DOI, and the arXiv identifier. GitHub commit search
returned zero commits for those identifiers. Sourcegraph returned zero code
matches for `BF-COIE` and `COIE.Encode`, including archived repositories and
forks. Its sole exact-title hit was arXiv metadata in a third-party statistics
repository, not source code.

Wayback Machine queries were attempted for author and GitHub pages, but the
CDX service returned HTTP 429 throughout this audit. No archived code URL was
found through independent web indexing. This is a search limitation and is
recorded rather than treated as proof that deleted private/unindexed code never
existed.

## What the recovered paper evidence does establish

The paper establishes these implementation facts:

- BFV via PySEAL and Microsoft SEAL 3.6;
- single-server PIR via SealPIR;
- single-threaded computation with no batching optimization in the main
  computation benchmark;
- Algorithm 7 timing begins with the server already holding encrypted match
  bits and measures steps 4 onward;
- `fp=16`, `lambda=40`, and `eta=2`;
- no homomorphic multiplication in BF-COIE Encode;
- 16-bit database items in the paper's own evaluation;
- `n=10000, s=16` reported about 16.7 seconds for the full post-Match Fetch
  boundary on the authors' machine;
- the reported unbatched communication structure at `n=10000, s=16` is 1323
  FHE ciphertexts plus 32 PIR requests.

The recovered evidence does **not** establish:

- the authors' BFV polynomial modulus degree;
- the exact coefficient-modulus chain;
- the exact plaintext modulus;
- the exact PySEAL fork/patch set used with SEAL 3.6;
- hash-function implementation, seeds, or integer rounding code;
- the exact SealPIR commit or parameterization selected by the authors;
- experiment scripts or raw samples.

## Gate outcome and next permitted action

Authentic BF-COIE source was not recovered. A matched local implementation is
permitted only under the label **BF-COIE reimplementation**.

The closest reproducible historical dependency pair is:

- Microsoft SEAL 3.6.6, tag commit
  `d045f1beff96dff0fccc7fa0c5acb1493a65338c`;
- Microsoft SealPIR commit
  `c7916baa05b43a5c96b8de094232a64f168515a2`, whose README and CMake files
  target SEAL 3.6.6.

Before benchmark implementation, that pair must build and run unchanged on
the benchmark host. If it cannot, the task must stop before substituting a
different PIR or HE stack. Direct Microsoft SEAL C++ is the approved fallback
for unavailable PySEAL bindings, but SealPIR itself may not be replaced without
new approval.
