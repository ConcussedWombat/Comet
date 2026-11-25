#!/usr/bin/env python3
"""Summarize hair-shader slow-command notes; no game assets are read."""
import argparse
from collections import Counter
from dataclasses import dataclass, field
import re
import statistics
from pathlib import Path

TARGETS = ("c47e03e0e3946741", "e35b7de6a9d2bfd4")
NOTE = re.compile(r"slow list command,\s*(\d+(?:\.\d+)?)\s*ms:\s*(.*)", re.I)
STAGE = re.compile(r"(?:^|\|\s*)(?:#)?([0-9a-f]{16}):\s*(.*?)(?=\s+\|\s*(?:#)?[0-9a-f]{16}:|$)", re.I)
WORD = re.compile(r"(?<![\w])u(\d+)=(0x[0-9a-f]+)(?:\s+\[heap\s+\d+:\s*(.*?)\])?", re.I)


@dataclass
class Samples:
    ms: list[float] = field(default_factory=list)
    high_byte: Counter = field(default_factory=Counter)
    u4_values: Counter = field(default_factory=Counter)
    annotations: Counter = field(default_factory=Counter)
    missing_u42: int = 0
    missing_u4: int = 0


def analyze(lines):
    results = {key: Samples() for key in TARGETS}
    for line in lines:
        match = NOTE.search(line)
        if not match:
            continue
        elapsed, body = float(match[1]), match[2]
        for stage in STAGE.finditer(body):
            key = stage[1].lower()
            if key not in results:
                continue
            sample = results[key]
            sample.ms.append(elapsed)
            words = {int(m[1]): (int(m[2], 16), m[3]) for m in WORD.finditer(stage[2])}
            if 42 in words:
                sample.high_byte[(words[42][0] >> 24) & 0xff] += 1
            else:
                sample.missing_u42 += 1
            if 4 in words:
                value, description = words[4]
                sample.u4_values[value] += 1
                annotation = "not annotated" if description is None else (
                    "null annotation" if re.search(r"\bnull\b", description, re.I) else "non-null annotation")
                sample.annotations[annotation] += 1
            else:
                sample.missing_u4 += 1
    return results


def report(results):
    print("Sampled slow commands only; NOT all dispatches, frame averages, or verified actual bindings.")
    print("u42 >> 24 is a candidate loop count only for a shader whose DXIL proves that mapping.")
    for key, sample in results.items():
        print(f"\n{key}: {len(sample.ms)} sampled commands")
        if not sample.ms:
            continue
        print(f"  GPU ms: min={min(sample.ms):.1f}, median={statistics.median(sample.ms):.1f}, max={max(sample.ms):.1f}")
        print(f"  u42 high-byte histogram: {dict(sorted(sample.high_byte.items()))}; missing={sample.missing_u42}")
        print(f"  u4 observed values: { {hex(k): v for k, v in sorted(sample.u4_values.items())} }; missing={sample.missing_u4}")
        print(f"  u4 heap annotations: {dict(sample.annotations)}")


def self_test():
    a, b = TARGETS
    rows = [
        f"WARN slow list command, 2.5 ms: dispatch [compute {a}] | {a}: u4=0xa67100 [heap 340872: null] u42=0x11000000 root4=0xabc",
        f"WARN slow list command, 1.0 ms: dispatch [compute {a}] | {a}: u4=0xa67100 [heap 340872: raw UAV] u42=0xffabcdef",
        f"WARN slow list command, 3.0 ms: dispatch [compute {b}] | {b}: u4=0x1",
        f"WARN slow list command, 9.0 ms: dispatch [compute {b}] | {b}: u42=0x00000000",
        f"INFO dispatch [compute {a}] | {a}: u42=0x99000000",
        "WARN slow list command, 1.0 ms: dispatch [compute 0000000000000000] | 0000000000000000: u42=0xffffffff",
    ]
    x, y = (analyze(rows)[k] for k in TARGETS)
    assert x.ms == [2.5, 1.0] and x.high_byte == {17: 1, 255: 1}
    assert x.u4_values == {0xa67100: 2} and x.annotations == {"null annotation": 1, "non-null annotation": 1}
    assert y.ms == [3.0, 9.0] and y.high_byte == {0: 1}
    assert y.missing_u42 == 1 and y.missing_u4 == 1
    assert y.annotations == {"not annotated": 1}
    print("Synthetic log parsing tests passed.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", nargs="?", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
    if args.log:
        with args.log.open(encoding="utf-8-sig", errors="replace") as log:
            report(analyze(log))
    elif not args.self_test:
        parser.error("provide a diagnostic log or --self-test")


if __name__ == "__main__":
    main()
