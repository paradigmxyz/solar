#!/usr/bin/env python3
"""Reduce a runtime failure while preserving replay.

This is a Solidity-harness-aware reducer, not a generic text shrinker. It keeps
the generated `setup/run/observe` harness shape intact, produces candidate
sources, and accepts a candidate only when `run_source_runtime.py
--replay-failure` still reproduces the original mismatch.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import subprocess
import tempfile
from dataclasses import dataclass
from typing import Any, Self


RUN_RE = re.compile(
    r"(?P<prefix>function\s+run\s*\([^)]*\)\s+external\s+returns\s*\(uint256\s+r\)\s*\{\s*unchecked\s*\{\s*)"
    r"(?P<body>.*?)"
    r"(?P<suffix>\s*value\s*=\s*r\s*;\s*values\s*\[\s*a\s*&\s*7\s*\]\s*=\s*r\s*;\s*emit\s+Seen\s*\(\s*1\s*,\s*r\s*\)\s*;\s*return\s+r\s*;\s*\}\s*\})",
    re.DOTALL,
)

@dataclass
class Stats:
    attempts: int = 0
    accepted: int = 0


@dataclass(frozen=True)
class RunParts:
    match: re.Match[str]
    body: str

    @classmethod
    def parse(cls, source: str) -> Self | None:
        match = RUN_RE.search(source)
        if match is None:
            return None
        return cls(match, match.group("body"))


class Reducer:
    def __init__(self, args: argparse.Namespace, failure: dict[str, Any]) -> None:
        self.args = args
        self.failure = failure
        self.stats = Stats()
        self.seen_sources: set[str] = set()

    def reduce(self, source: str) -> str:
        if not self.reproduces(source):
            raise RuntimeError("input failure does not reproduce")
        self.seen_sources.add(source)

        changed = True
        rounds = 0
        while changed and self.stats.attempts < self.args.max_attempts:
            rounds += 1
            if rounds > self.args.max_rounds:
                break
            changed = False
            for reducer in (
                self.reduce_statement_chunks,
                self.simplify_structured_items,
                self.simplify_calls,
                self.simplify_assignments,
                self.simplify_index_expressions,
                self.simplify_literals,
                self.remove_unused_helpers,
            ):
                next_source = reducer(source)
                if next_source != source:
                    source = next_source
                    changed = True
                    break
        return source

    def reduce_statement_chunks(self, source: str) -> str:
        parts = RunParts.parse(source)
        if parts is None:
            return source

        items = split_top_level_items(parts.body)
        if len(items) == 1:
            candidate = replace_body(source, parts.match, "r = value;")
            if self.try_candidate(candidate, "replace single statement body"):
                return candidate
            return source
        if not items:
            return source

        granularity = 2
        while len(items) >= 2:
            chunk_size = max(1, len(items) // granularity)
            changed = False
            for start in range(0, len(items), chunk_size):
                candidate_items = items[:start] + items[start + chunk_size:]
                candidate = replace_body(source, parts.match, body_or_default(candidate_items))
                if self.try_candidate(candidate, f"remove statement chunk {start}:{start + chunk_size}"):
                    source = candidate
                    parts = RunParts.parse(source)
                    if parts is None:
                        return source
                    items = split_top_level_items(parts.body)
                    granularity = 2
                    changed = True
                    break
            if changed:
                continue
            if granularity >= len(items):
                break
            granularity = min(len(items), granularity * 2)
        return source

    def simplify_structured_items(self, source: str) -> str:
        parts = RunParts.parse(source)
        if parts is None:
            return source

        items = split_top_level_items(parts.body)
        for index, item in enumerate(items):
            for replacement in structured_replacements(item):
                candidate_items = list(items)
                candidate_items[index] = replacement
                candidate = replace_body(source, parts.match, body_or_default(candidate_items))
                if self.try_candidate(candidate, f"simplify structured item {index}"):
                    return candidate
        return source

    def simplify_calls(self, source: str) -> str:
        return self.apply_text_replacements(source, call_replacements(source))

    def simplify_assignments(self, source: str) -> str:
        return self.apply_text_replacements(source, assignment_replacements(source))

    def simplify_index_expressions(self, source: str) -> str:
        return self.apply_text_replacements(source, index_replacements(source))

    def simplify_literals(self, source: str) -> str:
        return self.apply_text_replacements(source, literal_replacements(source))

    def remove_unused_helpers(self, source: str) -> str:
        for name in ("helper", "mix"):
            span = find_function_span(source, name)
            if span is None:
                continue
            start, end = span
            if re.search(rf"\b{name}\s*\(", source[:start] + source[end:]):
                continue
            candidate = source[:start] + "\n" + source[end:]
            if self.try_candidate(candidate, f"remove unused {name}"):
                return candidate
        return source

    def apply_text_replacements(self, source: str, replacements: list[tuple[int, int, str, str]]) -> str:
        for start, end, replacement, label in replacements:
            candidate = source[:start] + replacement + source[end:]
            if candidate != source and self.try_candidate(candidate, label):
                return candidate
        return source

    def try_candidate(self, source: str, label: str) -> bool:
        if self.stats.attempts >= self.args.max_attempts:
            return False
        if not is_plausible_source(source):
            return False
        if source in self.seen_sources:
            return False
        self.seen_sources.add(source)
        self.stats.attempts += 1
        if not self.reproduces(source):
            return False
        self.stats.accepted += 1
        if self.args.verbose:
            print(f"accepted: {label}")
        return True

    def reproduces(self, source: str) -> bool:
        with tempfile.TemporaryDirectory(prefix="solar-runtime-reduce-") as tmp:
            tmpdir = pathlib.Path(tmp)
            failure_path = tmpdir / "failure.json"
            candidate = dict(self.failure)
            candidate["source_text"] = source
            failure_path.write_text(json.dumps(candidate, sort_keys=True) + "\n")
            # Resolve the runner next to this file so the reducer also works
            # through the `fuzz/bin/solreduce` wrapper regardless of cwd.
            runner = pathlib.Path(__file__).resolve().parent / "run_source_runtime.py"
            result = subprocess.run(
                [
                    "python3",
                    str(runner),
                    "--replay-failure",
                    str(failure_path),
                    "--solc",
                    self.args.solc,
                    "--solar",
                    self.args.solar,
                    "--timeout",
                    str(self.args.timeout),
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=self.args.timeout + 5,
            )
        try:
            summary = json.loads(result.stdout.strip().splitlines()[-1])
        except (IndexError, json.JSONDecodeError):
            return False
        if result.returncode == 0 or summary.get("failures") != 1:
            return False
        replayed = summary.get("failure", {})
        return (
            replayed.get("kind") == self.failure.get("kind")
            and replayed.get("label") == self.failure.get("label")
            and replayed.get("calldata") == self.failure.get("calldata")
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--failure", type=pathlib.Path, required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    parser.add_argument("--solc", default="solc")
    parser.add_argument("--solar", default="target/debug/solar")
    parser.add_argument("--timeout", type=float, default=20.0)
    parser.add_argument("--max-attempts", type=int, default=512)
    parser.add_argument("--max-rounds", type=int, default=12)
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    failure = json.loads(args.failure.read_text())
    source = failure["source_text"]
    reducer = Reducer(args, failure)
    reduced = reducer.reduce(source)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(reduced)
    reduced_failure = dict(failure)
    reduced_failure["source_text"] = reduced
    args.out.with_suffix(".json").write_text(json.dumps(reduced_failure, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "attempts": reducer.stats.attempts,
        "accepted": reducer.stats.accepted,
        "input_bytes": len(source),
        "output_bytes": len(reduced),
        "out": str(args.out),
    }, separators=(",", ":")))
    return 0


def split_top_level_items(body: str) -> list[str]:
    items = []
    start = 0
    depth = 0
    index = 0
    while index < len(body):
        char = body[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth = max(depth - 1, 0)
            if depth == 0 and not next_nonspace_starts_with(body, index + 1, "else"):
                item = body[start:index + 1].strip()
                if item:
                    items.append(item)
                start = index + 1
        elif char == ";" and depth == 0:
            item = body[start:index + 1].strip()
            if item:
                items.append(item)
            start = index + 1
        index += 1

    tail = body[start:].strip()
    if tail:
        items.append(tail)
    return items


def next_nonspace_starts_with(text: str, index: int, prefix: str) -> bool:
    while index < len(text) and text[index].isspace():
        index += 1
    return text.startswith(prefix, index)


def structured_replacements(item: str) -> list[str]:
    replacements = []
    if_match = match_if_else(item)
    if if_match is not None:
        replacements.extend([
            if_match["then"].strip(),
            if_match["else"].strip(),
            "r = 0;",
            "r = value;",
        ])

    for_match = match_for(item)
    if for_match is not None:
        init = for_match["init"].strip()
        body = for_match["body"].strip()
        replacements.extend([
            body,
            f"{init};",
            f"{init}; r = value;",
            re.sub(r"<\s*[^;]+", "< 1", item, count=1),
            re.sub(r"<\s*[^;]+", "< 0", item, count=1),
        ])

    if "if (" in item and " else " not in item:
        simple = match_if(item)
        if simple is not None:
            replacements.extend([simple["then"].strip(), ""])
    return [replacement for replacement in replacements if replacement.strip()]


def match_if_else(item: str) -> dict[str, str] | None:
    prefix = "if"
    start = item.find(prefix)
    if start < 0:
        return None
    cond_start = item.find("(", start)
    cond_end = matching_delimiter(item, cond_start, "(", ")")
    then_start = item.find("{", cond_end)
    then_end = matching_delimiter(item, then_start, "{", "}")
    else_pos = item.find("else", then_end)
    if min(cond_start, cond_end, then_start, then_end, else_pos) < 0:
        return None
    else_start = item.find("{", else_pos)
    else_end = matching_delimiter(item, else_start, "{", "}")
    if else_start < 0 or else_end < 0:
        return None
    return {
        "condition": item[cond_start + 1:cond_end],
        "then": item[then_start + 1:then_end],
        "else": item[else_start + 1:else_end],
    }


def match_if(item: str) -> dict[str, str] | None:
    start = item.find("if")
    cond_start = item.find("(", start)
    cond_end = matching_delimiter(item, cond_start, "(", ")")
    then_start = item.find("{", cond_end)
    then_end = matching_delimiter(item, then_start, "{", "}")
    if min(start, cond_start, cond_end, then_start, then_end) < 0:
        return None
    return {
        "condition": item[cond_start + 1:cond_end],
        "then": item[then_start + 1:then_end],
    }


def match_for(item: str) -> dict[str, str] | None:
    start = item.find("for")
    header_start = item.find("(", start)
    header_end = matching_delimiter(item, header_start, "(", ")")
    body_start = item.find("{", header_end)
    body_end = matching_delimiter(item, body_start, "{", "}")
    if min(start, header_start, header_end, body_start, body_end) < 0:
        return None
    header = item[header_start + 1:header_end]
    parts = header.split(";")
    if len(parts) != 3:
        return None
    return {
        "init": parts[0],
        "condition": parts[1],
        "step": parts[2],
        "body": item[body_start + 1:body_end],
    }


def call_replacements(source: str) -> list[tuple[int, int, str, str]]:
    replacements = []
    for name in ("helper", "mix"):
        for start, end, args in find_calls(source, name):
            if name == "helper" and len(args) == 1:
                replacements.extend([
                    (start, end, parenthesize(args[0]), "inline helper"),
                    (start, end, "0", "helper to zero"),
                    (start, end, "a", "helper to a"),
                    (start, end, "value", "helper to value"),
                ])
            elif name == "mix" and len(args) == 2:
                replacements.extend([
                    (start, end, parenthesize(args[0]), "mix to left"),
                    (start, end, parenthesize(args[1]), "mix to right"),
                    (start, end, f"({args[0]} ^ {args[1]})", "mix to xor"),
                    (start, end, "0", "mix to zero"),
                ])
    return replacements


def index_replacements(source: str) -> list[tuple[int, int, str, str]]:
    replacements = []
    for match in re.finditer(r"values\s*\[(?P<index>[^\]]+)\]", source):
        index = match.group("index").strip()
        if index != "0":
            replacements.append((match.start("index"), match.end("index"), "0", "mapping key to zero"))
    for match in re.finditer(r"data\s*\[(?P<index>[^\]]+)\]", source):
        index = match.group("index").strip()
        if index != "0":
            replacements.append((match.start("index"), match.end("index"), "0", "data index to zero"))
    for match in re.finditer(r"xs\s*\[(?P<index>[^\]]+)\]", source):
        index = match.group("index").strip()
        if index != "0":
            replacements.append((match.start("index"), match.end("index"), "0", "array index to zero"))
    return replacements


def literal_replacements(source: str) -> list[tuple[int, int, str, str]]:
    replacements = []
    for match in re.finditer(r"\b(?:0x[0-9A-Fa-f]+|[1-9][0-9]*)\b", source):
        literal = match.group(0)
        if literal in {"0", "1"}:
            continue
        replacements.extend([
            (match.start(), match.end(), "0", f"{literal} to zero"),
            (match.start(), match.end(), "1", f"{literal} to one"),
        ])
        if literal.startswith("0x") or int(literal) > 2:
            replacements.append((match.start(), match.end(), "2", f"{literal} to two"))
    return replacements


def assignment_replacements(source: str) -> list[tuple[int, int, str, str]]:
    replacements = []
    for match in re.finditer(r"\br\s*=\s*(?P<expr>[^;{}]+);", source):
        expr = match.group("expr").strip()
        for replacement in ("0", "1", "a", "b", "value"):
            if expr != replacement:
                replacements.append((match.start("expr"), match.end("expr"), replacement, f"r assignment to {replacement}"))
    for match in re.finditer(r"\br\s*\+=\s*(?P<expr>[^;{}]+);", source):
        replacements.extend([
            (match.start(), match.end(), "r += 0;", "remove r addend"),
            (match.start("expr"), match.end("expr"), "1", "r addend to one"),
        ])
    for match in re.finditer(r"\buint256\s+(?P<name>limit|key)\s*=\s*(?P<expr>[^;{}]+);", source):
        expr = match.group("expr").strip()
        for replacement in ("0", "1", "a & 7"):
            if expr != replacement:
                replacements.append((match.start("expr"), match.end("expr"), replacement, f"{match.group('name')} to {replacement}"))
    return replacements


def find_function_span(source: str, name: str) -> tuple[int, int] | None:
    match = re.search(rf"\n\s*function\s+{name}\s*\(", source)
    if match is None:
        return None
    open_brace = source.find("{", match.end())
    close_brace = matching_delimiter(source, open_brace, "{", "}")
    if open_brace < 0 or close_brace < 0:
        return None
    end = close_brace + 1
    while end < len(source) and source[end] in " \t\r\n":
        end += 1
    return match.start(), end


def find_calls(source: str, name: str) -> list[tuple[int, int, list[str]]]:
    calls = []
    pattern = re.compile(rf"\b{name}\s*\(")
    for match in pattern.finditer(source):
        open_index = source.find("(", match.start())
        close_index = matching_delimiter(source, open_index, "(", ")")
        if close_index < 0:
            continue
        args = split_args(source[open_index + 1:close_index])
        calls.append((match.start(), close_index + 1, args))
    return calls


def split_args(text: str) -> list[str]:
    args = []
    start = 0
    depth = 0
    for index, char in enumerate(text):
        if char == "(":
            depth += 1
        elif char == ")":
            depth = max(depth - 1, 0)
        elif char == "," and depth == 0:
            args.append(text[start:index].strip())
            start = index + 1
    tail = text[start:].strip()
    if tail:
        args.append(tail)
    return args


def matching_delimiter(text: str, start: int, open_char: str, close_char: str) -> int:
    if start < 0 or start >= len(text) or text[start] != open_char:
        return -1
    depth = 0
    for index in range(start, len(text)):
        if text[index] == open_char:
            depth += 1
        elif text[index] == close_char:
            depth -= 1
            if depth == 0:
                return index
    return -1


def parenthesize(expr: str) -> str:
    return expr if expr.strip().isidentifier() else f"({expr})"


def body_or_default(items: list[str]) -> str:
    body = " ".join(item.strip() for item in items if item.strip()).strip()
    if "r =" not in body and "r +=" not in body:
        body = f"{body} r = value;".strip()
    return body or "r = value;"


def replace_body(source: str, match: re.Match[str], body: str) -> str:
    return source[:match.start()] + match.group("prefix") + body + match.group("suffix") + source[match.end():]


def is_plausible_source(source: str) -> bool:
    return (
        "contract FandangoRuntime" in source
        and "function setup" in source
        and "function run" in source
        and "function observe" in source
        and source.count("{") == source.count("}")
    )


if __name__ == "__main__":
    raise SystemExit(main())
