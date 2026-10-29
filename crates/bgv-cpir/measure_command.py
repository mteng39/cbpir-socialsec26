#!/usr/bin/env python3
"""Run one command and report elapsed time and child peak RSS on Linux."""

from __future__ import annotations

import argparse
import json
import resource
import subprocess
import time
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--stdout", type=Path, required=True)
    parser.add_argument("--stderr", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.command:
        parser.error("a command is required after --")
    if args.command[0] == "--":
        args.command = args.command[1:]
    if not args.command:
        parser.error("a command is required after --")

    args.stdout.parent.mkdir(parents=True, exist_ok=True)
    args.stderr.parent.mkdir(parents=True, exist_ok=True)
    started = time.perf_counter_ns()
    with args.stdout.open("wb") as stdout, args.stderr.open("wb") as stderr:
        completed = subprocess.run(
            args.command,
            stdout=stdout,
            stderr=stderr,
            check=False,
        )
    elapsed_ns = time.perf_counter_ns() - started
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    print(
        json.dumps(
            {
                "schema": "linux-child-resource-v1",
                "classification": "measured-local-experiment",
                "command": args.command,
                "exit_code": completed.returncode,
                "elapsed_ns": elapsed_ns,
                "peak_rss_kib": usage.ru_maxrss,
                "peak_rss_semantics": (
                    "Linux ru_maxrss for child processes of this fresh wrapper process"
                ),
                "stdout_path": str(args.stdout),
                "stderr_path": str(args.stderr),
            },
            indent=2,
        )
    )
    return completed.returncode


if __name__ == "__main__":
    raise SystemExit(main())
