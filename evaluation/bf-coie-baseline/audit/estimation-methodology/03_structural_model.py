#!/usr/bin/env python3
"""Paper-only structural model for BF-COIE.

This module models Algorithms 1, 2, and the post-Match part of Algorithm 7.
It contains no HE implementation, latency constants, or CBPIR measurements.
Ambiguous integer conventions must be supplied explicitly.
"""

from __future__ import annotations

import argparse
import json
import math
from typing import Any, Dict, Iterable, NamedTuple


ELL_ROUNDINGS = ("floor", "nearest", "ceil")
HIERARCHY_CONVENTIONS = ("exact", "floor", "ceil", "pad_to_power_of_two")


class StructuralModel(NamedTuple):
    n: int
    effective_n: int
    s: int
    fp: int
    eta: int
    lambda_bits: int
    hierarchy_convention: str
    hierarchy_t_raw: float
    hierarchy_t: int
    bf_levels: int
    m: int
    ell_raw: float
    ell_rounding: str
    ell_integer: int
    ell_exact_inequality_minimum: int
    compactness: int
    count_reduction_additions: int
    count_zero_accumulator_additions: int
    encode_hash_evaluations: int
    encode_accumulation_attempts: int
    encode_additions_lower_bound: int
    encode_additions_upper_bound: int
    ct_ct_multiplications: int
    decode_bf_check_bound: int
    encrypted_bf_outputs: int
    coie_decryptions: int
    pir_request_count: int
    interaction_rounds: int
    protocol_interactions: tuple[str, ...]
    semantic_domain: str

    def as_dict(self) -> Dict[str, Any]:
        return self._asdict()


def _is_power_of_two(value: int) -> bool:
    return value > 0 and value & (value - 1) == 0


def _next_power_of_two(value: int) -> int:
    if value <= 0:
        raise ValueError("value must be positive")
    return 1 << (value - 1).bit_length()


def _round_ell(value: float, convention: str) -> int:
    if convention == "floor":
        return math.floor(value)
    if convention == "nearest":
        # Explicit round-half-up. Python's built-in round uses ties-to-even.
        return math.floor(value + 0.5)
    if convention == "ceil":
        return math.ceil(value)
    raise ValueError(f"unknown ell rounding convention: {convention}")


def bloom_false_positive_probability(ell: int, s: int, eta: int) -> float:
    """Return the paper's Bloom-filter approximation for one membership check."""
    if ell <= 0 or s <= 0 or eta <= 0:
        raise ValueError("ell, s, and eta must be positive")
    return (1.0 - math.exp(-(eta * s) / ell)) ** eta


def bloom_false_positive_bound_holds(ell: int, s: int, eta: int, m: int) -> bool:
    if m <= 0:
        raise ValueError("m must be positive")
    return bloom_false_positive_probability(ell, s, eta) <= (1.0 / m)


def exact_inequality_minimum_ell(s: int, eta: int, m: int) -> int:
    """Smallest integer ell satisfying the paper's displayed BF inequality."""
    if s <= 0 or eta <= 0 or m <= 1:
        raise ValueError("s and eta must be positive and m must exceed one")
    threshold = m ** (-1.0 / eta)
    real_minimum = (eta * s) / (-math.log(1.0 - threshold))
    candidate = max(1, math.ceil(real_minimum))
    # Floating-point guard at an exact integer boundary.
    while not bloom_false_positive_bound_holds(candidate, s, eta, m):
        candidate += 1
    while candidate > 1 and bloom_false_positive_bound_holds(candidate - 1, s, eta, m):
        candidate -= 1
    return candidate


def _hierarchy(n: int, s: int, convention: str) -> tuple[int, float, int]:
    ratio = n / (2 * s)
    raw = math.log2(ratio)
    exact_supported = n % (2 * s) == 0 and _is_power_of_two(n // (2 * s))

    if convention == "exact":
        if not exact_supported:
            raise ValueError(
                "paper simplification does not define this input; choose an explicit "
                "non-power-of-two hierarchy convention"
            )
        return n, raw, int(raw)
    if convention == "floor":
        return n, raw, math.floor(raw)
    if convention == "ceil":
        return n, raw, math.ceil(raw)
    if convention == "pad_to_power_of_two":
        effective_n = _next_power_of_two(n)
        padded_ratio = effective_n / (2 * s)
        padded_raw = math.log2(padded_ratio)
        if not padded_raw.is_integer():
            raise ValueError("padding n alone did not create an integral hierarchy")
        return effective_n, raw, int(padded_raw)
    raise ValueError(f"unknown hierarchy convention: {convention}")


def semantic_domain(n: int, s: int) -> str:
    """Classify a cell without treating an asymptotic premise as a syntax rule."""
    if s <= 0 or n <= 0 or 2 * s > n:
        return "problematic"
    # Section 7 evaluates n from 1,000 to 100,000 and s from 8 to 128.
    if 1_000 <= n <= 100_000 and 8 <= s <= 128:
        return "directly supported"
    return "structurally supported but outside original experimental range"


def build_model(
    n: int,
    s: int,
    fp: int = 16,
    eta: int = 2,
    lambda_bits: int = 40,
    ell_rounding: str = "ceil",
    hierarchy_convention: str = "exact",
) -> StructuralModel:
    if n <= 0:
        raise ValueError("n must be positive")
    if s <= 0:
        raise ValueError("s must be positive")
    if s > n:
        raise ValueError("s must not exceed n")
    if 2 * s > n:
        raise ValueError("2s must not exceed n for Algorithm 2's top level")
    if fp < 0:
        raise ValueError("fp must be non-negative")
    if eta <= 0:
        raise ValueError("eta must be positive")
    if lambda_bits <= 0:
        raise ValueError("lambda must be positive")
    if ell_rounding not in ELL_ROUNDINGS:
        raise ValueError(f"ell_rounding must be one of {ELL_ROUNDINGS}")
    if hierarchy_convention not in HIERARCHY_CONVENTIONS:
        raise ValueError(f"hierarchy_convention must be one of {HIERARCHY_CONVENTIONS}")

    effective_n, hierarchy_t_raw, hierarchy_t = _hierarchy(n, s, hierarchy_convention)
    levels = hierarchy_t + 1
    m = max(2 * s, s + 2 * fp)
    ell_raw = eta * s * (m ** (1.0 / eta))
    ell_integer = _round_ell(ell_raw, ell_rounding)
    exact_minimum = exact_inequality_minimum_ell(s, eta, m)
    compactness = levels * ell_integer

    # Algorithm 1 loops over the original encrypted vector. A padding policy would
    # need encrypted zeros; that implementation choice is intentionally not hidden.
    encode_n = effective_n if hierarchy_convention == "pad_to_power_of_two" else n
    attempts = eta * encode_n * levels

    return StructuralModel(
        n=n,
        effective_n=effective_n,
        s=s,
        fp=fp,
        eta=eta,
        lambda_bits=lambda_bits,
        hierarchy_convention=hierarchy_convention,
        hierarchy_t_raw=hierarchy_t_raw,
        hierarchy_t=hierarchy_t,
        bf_levels=levels,
        m=m,
        ell_raw=ell_raw,
        ell_rounding=ell_rounding,
        ell_integer=ell_integer,
        ell_exact_inequality_minimum=exact_minimum,
        compactness=compactness,
        count_reduction_additions=n - 1,
        count_zero_accumulator_additions=n,
        encode_hash_evaluations=attempts,
        encode_accumulation_attempts=attempts,
        # A literal nil cell is assigned without addition. The number of occupied
        # cells depends on sampled hashes, so the paper gives a range, not one count.
        encode_additions_lower_bound=max(0, attempts - compactness),
        encode_additions_upper_bound=attempts,
        ct_ct_multiplications=0,
        # Top BF checks 2s indices. Each of the t lower levels has at most m
        # candidates under the paper's high-probability false-positive argument.
        decode_bf_check_bound=(2 * s) + (hierarchy_t * m),
        encrypted_bf_outputs=compactness,
        coie_decryptions=compactness,
        pir_request_count=s + fp,
        interaction_rounds=3,
        protocol_interactions=(
            "server sends encrypted count; client returns plaintext s",
            "server sends encrypted BF hierarchy; client decodes and pads indices",
            "client and server execute PIR for s+fp indices",
        ),
        semantic_domain=semantic_domain(n, s),
    )


def projected_index_sets(selected: Iterable[int], max_level: int) -> Dict[int, set[int]]:
    """Return I^k={ceil(i/2^k)} for the Section 4.2 hierarchy example."""
    selected_set = set(selected)
    if not selected_set or min(selected_set) <= 0:
        raise ValueError("selected indices must be non-empty and one-based")
    if max_level < 0:
        raise ValueError("max_level must be non-negative")
    return {
        level: {math.ceil(index / (2**level)) for index in selected_set}
        for level in range(max_level, -1, -1)
    }


def paper_figure5_count_candidates(
    n: int = 10000,
    s: int = 16,
    fp: int = 16,
    eta: int = 2,
) -> Dict[str, int]:
    """Enumerate documented structural readings; do not select one post hoc."""
    candidates: Dict[str, int] = {}
    for hierarchy in ("floor", "ceil", "pad_to_power_of_two"):
        for rounding in ELL_ROUNDINGS:
            result = build_model(
                n=n,
                s=s,
                fp=fp,
                eta=eta,
                ell_rounding=rounding,
                hierarchy_convention=hierarchy,
            )
            base = f"paper_sufficient/{hierarchy}/{rounding}"
            candidates[f"{base}/BF_only"] = result.compactness
            candidates[f"{base}/count_plus_BF"] = 1 + result.compactness

        exact_result = build_model(
            n=n,
            s=s,
            fp=fp,
            eta=eta,
            ell_rounding="ceil",
            hierarchy_convention=hierarchy,
        )
        exact_c = exact_result.bf_levels * exact_result.ell_exact_inequality_minimum
        base = f"exact_inequality_minimum/{hierarchy}"
        candidates[f"{base}/BF_only"] = exact_c
        candidates[f"{base}/count_plus_BF"] = 1 + exact_c
    return candidates


def _main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--n", type=int, required=True)
    parser.add_argument("--s", type=int, required=True)
    parser.add_argument("--fp", type=int, default=16)
    parser.add_argument("--eta", type=int, default=2)
    parser.add_argument("--lambda-bits", type=int, default=40)
    parser.add_argument("--ell-rounding", choices=ELL_ROUNDINGS, default="ceil")
    parser.add_argument(
        "--hierarchy-convention",
        choices=HIERARCHY_CONVENTIONS,
        default="exact",
    )
    args = parser.parse_args()
    result = build_model(
        n=args.n,
        s=args.s,
        fp=args.fp,
        eta=args.eta,
        lambda_bits=args.lambda_bits,
        ell_rounding=args.ell_rounding,
        hierarchy_convention=args.hierarchy_convention,
    )
    print(json.dumps(result.as_dict(), indent=2))


if __name__ == "__main__":
    _main()
