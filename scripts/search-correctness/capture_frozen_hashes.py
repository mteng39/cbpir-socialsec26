#!/usr/bin/env python3
"""Capture stable SHA-256 manifests for immutable benchmark evidence."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
FROZEN_PATHS = (
    Path("evaluation/formal-benchmark/frozen-inputs"),
    Path("evaluation/formal-benchmark/raw"),
    Path("evaluation/formal-benchmark/results"),
)


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def manifest_lines() -> list[str]:
    lines: list[str] = []
    for relative_root in FROZEN_PATHS:
        directory = ROOT / relative_root
        for path in sorted(
            item
            for item in directory.rglob("*")
            if item.is_file()
            and "__pycache__" not in item.parts
            and item.suffix not in {".pyc", ".pyo"}
        ):
            relative = path.relative_to(ROOT)
            lines.append(f"{sha256(path)}  {relative.as_posix()}")
    return lines


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    output = args.output if args.output.is_absolute() else ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(manifest_lines()) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
