#!/usr/bin/env python3
"""Generate synthetic Solidity files for compile-time profiling."""

from __future__ import annotations

import argparse
from pathlib import Path


HEADER = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.0;\n"

SIZE_CONFIGS = {
    "small": ("small", 100, 10, 5),
    "medium": ("medium", 1000, 50, 10),
    "large": ("large", 10000, 200, 20),
}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sizes", choices=(*SIZE_CONFIGS, "all"), default="all")
    args = parser.parse_args()

    output_dir = Path(__file__).resolve().parents[1] / "testdata" / "repros"
    output_dir.mkdir(parents=True, exist_ok=True)

    configs = SIZE_CONFIGS.values() if args.sizes == "all" else (SIZE_CONFIGS[args.sizes],)
    for size_name, n_symbols, n_depth, n_types in configs:
        print(f"Generating {size_name} repros (n={n_symbols}, depth={n_depth}, types={n_types})...")
        generate_many_symbols(output_dir, size_name, n_symbols)
        generate_many_functions(output_dir, size_name, n_symbols)
        generate_deep_nesting(output_dir, size_name, n_depth)
        generate_many_types(output_dir, size_name, n_types * 10)
        generate_large_literals(output_dir, size_name, n_symbols)
        generate_many_storage(output_dir, size_name, n_symbols)
        generate_many_events(output_dir, size_name, n_symbols // 10)
        generate_complex_inheritance(output_dir, size_name, min(n_depth, 50))
        generate_many_mappings(output_dir, size_name, n_symbols // 10)
        generate_many_modifiers(output_dir, size_name, n_symbols // 10)

    print(f"\nGenerated repros in: {output_dir}")
    print("\nRun benchmarks with:")
    print("  cargo bench -p solar-bench --bench compile_time")


def source(lines: list[str]) -> str:
    return HEADER + "\n".join(lines) + "\n"


def generate_many_symbols(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{", "function c()public pure returns(uint r){"]
    lines.extend(f"uint v{i}={i};" for i in range(n))
    lines.append("r=" + "+".join(f"v{i}" for i in range(min(n, 100))) + ";")
    lines.extend(("}", "}"))
    write_file(directory, f"many_symbols_{size}.sol", source(lines))


def generate_many_functions(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{"]
    lines.extend(f"function f{i}(uint x)public pure returns(uint r){{r=x+{i};}}" for i in range(n))
    lines.append("}")
    write_file(directory, f"many_functions_{size}.sol", source(lines))


def generate_deep_nesting(directory: Path, size: str, depth: int) -> None:
    nested = ["function f(uint x)public pure returns(uint r){r=x;"]
    nested.extend(f"if(r>{i}){{r=r+1;" for i in range(depth))
    nested.append("}" * depth)
    nested.append("}")

    loop_depth = min(depth, 10)
    loops = ["function g(uint n)public pure returns(uint r){"]
    loops.extend(f"for(uint i{i}=0;i{i}<n;i{i}++){{" for i in range(loop_depth))
    loops.append("r+=1;")
    loops.append("}" * loop_depth)
    loops.append("}")

    write_file(directory, f"deep_nesting_{size}.sol", source(["contract C{", *nested, *loops, "}"]))


def generate_many_types(directory: Path, size: str, n: int) -> None:
    lines: list[str] = []
    lines.extend(f"struct S{i}{{uint a;address b;bytes32 c;}}" for i in range(n))
    lines.extend(f"enum E{i}{{A,B,C,D}}" for i in range(n))
    lines.extend(f"type T{i} is uint;" for i in range(n))
    lines.extend(f"error X{i}(uint c);" for i in range(n))
    lines.append("contract C{")
    for i in range(min(n, 100)):
        lines.append(f"S{i} public s{i};")
        lines.append(f"E{i} public e{i};")
    lines.append("}")
    write_file(directory, f"many_types_{size}.sol", source(lines))


def generate_large_literals(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{"]
    for i in range(n):
        value = (i * 12345678901234567890) % ((2**128 - 1) // 2)
        lines.append(f"uint public constant C{i}={value:#x};")

    lines.append("function f()public pure returns(uint r){")
    lines.extend(f"r+=C{i};" for i in range(min(n, 500)))
    lines.append("}")

    strings = ",".join(
        f'"String literal number {i} with some content to make it longer"' for i in range(min(n, 50))
    )
    lines.append(f"function g()public pure returns(string memory r){{r=string(abi.encodePacked({strings}));}}")
    lines.append("}")
    write_file(directory, f"large_literals_{size}.sol", source(lines))


def generate_many_storage(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{"]
    for i in range(n):
        match i % 5:
            case 0:
                lines.append(f"uint public u{i};")
            case 1:
                lines.append(f"address public a{i};")
            case 2:
                lines.append(f"bytes32 public h{i};")
            case 3:
                lines.append(f"bool public b{i};")
            case _:
                lines.append(f"uint128 public s{i};")

    lines.append("function f()public view returns(uint r){")
    for i in range(min(n, 200)):
        match i % 5:
            case 0:
                lines.append(f"r+=u{i};")
            case 1:
                lines.append(f"r+=uint(uint160(a{i}));")
            case 2:
                lines.append(f"r+=uint(h{i});")
            case 3:
                lines.append(f"r+=b{i}?1:0;")
            case _:
                lines.append(f"r+=s{i};")
    lines.append("}")
    lines.append("}")
    write_file(directory, f"many_storage_{size}.sol", source(lines))


def generate_many_events(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{"]
    lines.extend(f"event E{i}(address indexed a,uint indexed b,bytes32 c);" for i in range(n))
    lines.append("function f()public{")
    lines.extend(f"emit E{i}(msg.sender,{i},bytes32(uint({i})));" for i in range(min(n, 100)))
    lines.extend(("}", "}"))
    write_file(directory, f"many_events_{size}.sol", source(lines))


def generate_complex_inheritance(directory: Path, size: str, depth: int) -> None:
    lines = [
        "contract B{",
        "uint public v;",
        "function f()public virtual returns(uint r){r=0;}",
        "}",
    ]

    for prefix, root, field, func in (("L", "B", "l", "g"), ("R", "B", "r", "h")):
        for i in range(depth):
            parent = root if i == 0 else f"{prefix}{i - 1}"
            lines.extend(
                (
                    f"contract {prefix}{i} is {parent}{{",
                    f"uint public {field}{i};",
                    f"function {func}{i}()public pure returns(uint r){{r={i};}}",
                    "}",
                )
            )

    lines.extend(
        (
            f"contract D is L{depth - 1},R{depth - 1}{{",
            "function f()public pure override returns(uint r){r=42;}",
            "function d()public pure returns(uint r){",
        )
    )
    lines.extend(f"r+=g{i}()+h{i}();" for i in range(min(depth, 20)))
    lines.extend(("}", "}"))
    write_file(directory, f"complex_inheritance_{size}.sol", source(lines))


def generate_many_mappings(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{"]
    for i in range(n):
        match i % 4:
            case 0:
                lines.append(f"mapping(address=>uint) public m{i};")
            case 1:
                lines.append(f"mapping(uint=>mapping(address=>uint)) public n{i};")
            case 2:
                lines.append(f"mapping(bytes32=>address) public h{i};")
            case _:
                lines.append(f"mapping(address=>mapping(uint=>bool)) public d{i};")

    lines.append("function f(address a,uint v)public{")
    for i in range(min(n, 50)):
        match i % 4:
            case 0:
                lines.append(f"m{i}[a]=v;")
            case 1:
                lines.append(f"n{i}[v][a]=v;")
            case 2:
                lines.append(f"h{i}[keccak256(abi.encode(v))]=a;")
            case _:
                lines.append(f"d{i}[a][v]=true;")
    lines.extend(("}", "}"))
    write_file(directory, f"many_mappings_{size}.sol", source(lines))


def generate_many_modifiers(directory: Path, size: str, n: int) -> None:
    lines = ["contract C{", "address public o;", "uint public n;"]
    lines.extend(f'modifier m{i}(uint v){{require(v>{i},"m{i}");_;}}' for i in range(n))
    modifiers = " ".join(f"m{i}(x)" for i in range(min(n, 20)))
    lines.append(f"function f(uint x)public {modifiers} returns(uint r){{r=x;}}")
    lines.append("}")
    write_file(directory, f"many_modifiers_{size}.sol", source(lines))


def write_file(directory: Path, name: str, content: str) -> None:
    path = directory / name
    path.write_text(content)
    line_count = content.count("\n")
    print(f"  Generated: {name} ({len(content)} bytes, {line_count} lines)")


if __name__ == "__main__":
    main()
