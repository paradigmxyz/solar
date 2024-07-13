#!/usr/bin/env python3

import re
import sys

from tabulate import tabulate


def main():
    lines = sys.stdin.readlines()

    benchmark_re = re.compile(r"(\w+): (\d+) LoC, (\d+) bytes")
    benchmarks = []
    for line in lines:
        if line.strip() == "":
            break
        match = benchmark_re.match(line)
        if match:
            name, loc, bytes = match.groups()
            benchmarks.append((name, int(loc), int(bytes)))

    time = r"(\s*[\d\.]+ \w+)"
    data_re = re.compile(
        rf"parser/(\w+)/(\w+)/(\w+)\s*time:\s*\n?\s*\[{time}{time}{time}\]",
        flags=re.MULTILINE,
    )
    data = []
    for match in data_re.findall("\n".join(lines)):
        bench_name, parser, kind, _time1, time2, _time3 = match
        data.append([bench_name, parser, kind, time2, parse_time_s(time2)])

    parsers = list(sorted(set(x[1] for x in data)))

    # Solc patch to remove base overhead
    base_solc_ns = -1
    for [bench_name, parser, _, _, ns] in data:
        if bench_name == "empty" and parser == "solc":
            base_solc_ns = ns
            break
    if base_solc_ns == -1:
        raise ValueError("Couldn't find base solc time")
    base_solc_ns -= 1_000  # keep 1us
    for i, [bench_name, parser, _, _, ns] in enumerate(data):
        if parser == "solc":
            data[i][4] -= base_solc_ns
            data[i][3] = format_ns(data[i][4])

    for bench_name, loc, bytes in benchmarks:
        print(f"### {bench_name} ({loc} LoC, {bytes} bytes)")
        print()

        for kind in ["lex", "parse"]:
            table = []
            table.append(
                [
                    "Parser",
                    "Time",
                    "LoC/s",
                    "Bytes/s",
                ]
            )
            for parser in parsers:
                related = list(
                    filter(lambda x: x[0] == bench_name and x[1] == parser, data)
                )

                def parse(kind):
                    vals = [
                        (x[3], get_per_second(loc, x[4]), get_per_second(bytes, x[4]))
                        for x in related
                        if x[2] == kind
                    ]
                    return next(iter(vals), ("N/A", "N/A", "N/A"))

                time_s, loc_s, bytes_s = parse(kind)
                table.append(
                    [
                        parser,
                        time_s,
                        loc_s,
                        bytes_s,
                    ]
                )

            print("####", kind.capitalize())
            print(tabulate(table, headers="firstrow", tablefmt="pipe"))
            print()


def parse_time_s(time: str):
    value, unit = time.strip().split(" ")
    value = float(value)
    if unit == "s":
        return int(value * 1_000_000_000)
    elif unit == "ms":
        return int(value * 1_000_000)
    elif unit == "us" or unit == "µs":
        return int(value * 1_000)
    elif unit == "ns":
        return int(value * 1)
    else:
        raise ValueError(f"Unknown unit: {unit}")


def get_per_second(total: int, ns: int):
    if total == 0 or ns == -1:
        return "N/A"

    s = ns / 1_000_000_000
    return format_number(total / s)


def format_number(n: float):
    if n >= 1_000_000_000:
        n /= 1_000_000_000
        s = "B"
    elif n >= 1_000_000:
        n /= 1_000_000
        s = "M"
    elif n >= 1_000:
        n /= 1_000
        s = "K"
    else:
        s = ""
    return f"{n:.2f}{s}"


def format_ns(ns: int):
    if ns >= 1_000_000_000:
        ns /= 1_000_000_000
        s = "s"
    elif ns >= 1_000_000:
        ns /= 1_000_000
        s = "ms"
    elif ns >= 1_000:
        ns /= 1_000
        s = "µs"
    else:
        s = "ns"
    return f"{ns:f}"[:6] + " " + s


if __name__ == "__main__":
    main()
