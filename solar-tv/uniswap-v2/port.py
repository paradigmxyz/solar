#!/usr/bin/env python3
"""Writes the ^0.8 port of Uniswap v2-core that solar-tv and solar-verify target.

    solar-tv/uniswap-v2/port.py [--out DIR] [--v2-core DIR]

Clones Uniswap/v2-core at V2_CORE_COMMIT into --v2-core when that directory is missing
(default: target/uniswap-v2/v2-core), checks that it is that commit with no local changes, and
writes the port to --out (default: target/uniswap-v2/src): the contracts, libraries and
interfaces of its contracts/ directory at the same paths, each changed only by the edits below,
which README.md explains. v2-core is GPL-3.0, so this repository keeps the edits, not the
code; its LICENSE is copied next to the port. Every edit must apply exactly once, so a
different upstream fails here instead of silently changing the target.
"""

import argparse
import pathlib
import re
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
V2_CORE_URL = "https://github.com/Uniswap/v2-core.git"
V2_CORE_COMMIT = "6a9e7c97860676e0992f22a49665760444c1cdf5"
FILES = [
    "UniswapV2ERC20.sol",
    "UniswapV2Factory.sol",
    "UniswapV2Pair.sol",
    "libraries/Math.sol",
    "libraries/SafeMath.sol",
    "libraries/UQ112x112.sol",
    "interfaces/IERC20.sol",
    "interfaces/IUniswapV2Callee.sol",
    "interfaces/IUniswapV2ERC20.sol",
    "interfaces/IUniswapV2Factory.sol",
    "interfaces/IUniswapV2Pair.sol",
]

# (file, old, new): each old text must occur exactly once.
EDITS = [
    # ^0.8 rejects public on constructors.
    ("UniswapV2ERC20.sol", "constructor() public {", "constructor() {"),
    ("UniswapV2Pair.sol", "constructor() public {", "constructor() {"),
    (
        "UniswapV2Factory.sol",
        "constructor(address _feeToSetter) public {",
        "constructor(address _feeToSetter) {",
    ),
    # ^0.8 rejects converting a negative literal; these are the values 0.5.16 computed.
    ("UniswapV2ERC20.sol", "uint(-1)", "type(uint).max"),
    (
        "UniswapV2Pair.sol",
        "balance0 <= uint112(-1) && balance1 <= uint112(-1)",
        "balance0 <= type(uint112).max && balance1 <= type(uint112).max",
    ),
    # Yul builtins take parentheses in ^0.8.
    ("UniswapV2ERC20.sol", "chainId := chainid\n", "chainId := chainid()\n"),
    # The pair would need an override(IUniswapV2Pair, UniswapV2ERC20) on every ERC20 function.
    (
        "UniswapV2Pair.sol",
        "contract UniswapV2Pair is IUniswapV2Pair, UniswapV2ERC20 {",
        "contract UniswapV2Pair is UniswapV2ERC20 {",
    ),
    # ^0.8 rejects an event declared twice in one contract's hierarchy; the interfaces declare
    # these identically.
    (
        "UniswapV2ERC20.sol",
        (
            "    event Approval(address indexed owner, address indexed spender, uint value);\n"
            "    event Transfer(address indexed from, address indexed to, uint value);\n"
        ),
        "",
    ),
    (
        "UniswapV2Factory.sol",
        "    event PairCreated(address indexed token0, address indexed token1, address pair, uint);\n",
        "",
    ),
    # Overflow 0.5.16 relies on wraps.
    (
        "UniswapV2Pair.sol",
        (
            "        uint32 timeElapsed = blockTimestamp - blockTimestampLast; // overflow is desired\n"
            "        if (timeElapsed > 0 && _reserve0 != 0 && _reserve1 != 0) {\n"
            "            // * never overflows, and + overflow is desired\n"
            "            price0CumulativeLast += uint(UQ112x112.encode(_reserve1).uqdiv(_reserve0)) * timeElapsed;\n"
            "            price1CumulativeLast += uint(UQ112x112.encode(_reserve0).uqdiv(_reserve1)) * timeElapsed;\n"
            "        }\n"
        ),
        (
            "        unchecked {\n"
            "        uint32 timeElapsed = blockTimestamp - blockTimestampLast; // overflow is desired\n"
            "        if (timeElapsed > 0 && _reserve0 != 0 && _reserve1 != 0) {\n"
            "            // * never overflows, and + overflow is desired\n"
            "            price0CumulativeLast += uint(UQ112x112.encode(_reserve1).uqdiv(_reserve0)) * timeElapsed;\n"
            "            price1CumulativeLast += uint(UQ112x112.encode(_reserve0).uqdiv(_reserve1)) * timeElapsed;\n"
            "        }\n"
            "        }\n"
        ),
    ),
    # SafeMath checks the wrapped result itself, so it reverts with its own reason, as in 0.5.16.
    (
        "libraries/SafeMath.sol",
        "        require((z = x + y) >= x, 'ds-math-add-overflow');\n",
        "        unchecked { require((z = x + y) >= x, 'ds-math-add-overflow'); }\n",
    ),
    (
        "libraries/SafeMath.sol",
        "        require((z = x - y) <= x, 'ds-math-sub-underflow');\n",
        "        unchecked { require((z = x - y) <= x, 'ds-math-sub-underflow'); }\n",
    ),
    (
        "libraries/SafeMath.sol",
        "        require(y == 0 || (z = x * y) / y == x, 'ds-math-mul-overflow');\n",
        "        unchecked { require(y == 0 || (z = x * y) / y == x, 'ds-math-mul-overflow'); }\n",
    ),
]


def git(*args, cwd=None):
    p = subprocess.run(
        ["git", *args], cwd=cwd, capture_output=True, text=True, check=False
    )
    if p.returncode != 0:
        sys.exit(f"error: git {' '.join(args)} failed:\n{p.stderr}")
    return p.stdout.strip()


def checkout(dest):
    if not (dest / ".git").exists():
        dest.parent.mkdir(parents=True, exist_ok=True)
        git("clone", "-q", V2_CORE_URL, str(dest))
    if git("rev-parse", "HEAD", cwd=dest) != V2_CORE_COMMIT:
        git("checkout", "-q", V2_CORE_COMMIT, cwd=dest)
    if git("status", "--porcelain", cwd=dest):
        sys.exit(f"error: {dest} has local changes")


def port(v2_core, out):
    pragma = re.compile(r"pragma solidity (?:=0\.5\.16|>=0\.5\.0);")
    texts = {}
    for f in FILES:
        text = (v2_core / "contracts" / f).read_text()
        if len(pragma.findall(text)) != 1:
            sys.exit(f"error: {f} does not have one pragma to port")
        texts[f] = pragma.sub("pragma solidity ^0.8.0;", text)
    for f, old, new in EDITS:
        n = texts[f].count(old)
        if n != 1:
            sys.exit(f"error: an edit of {f} matches {n} times: {old[:60]!r}")
        texts[f] = texts[f].replace(old, new)
    if out.exists():
        shutil.rmtree(out)
    for f, text in texts.items():
        (out / f).parent.mkdir(parents=True, exist_ok=True)
        (out / f).write_text(text)
    shutil.copy(v2_core / "LICENSE", out / "LICENSE")


def main():
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "--out", type=pathlib.Path, default=ROOT / "target" / "uniswap-v2" / "src"
    )
    ap.add_argument(
        "--v2-core",
        type=pathlib.Path,
        default=ROOT / "target" / "uniswap-v2" / "v2-core",
    )
    args = ap.parse_args()
    checkout(args.v2_core.resolve())
    port(args.v2_core.resolve(), args.out.resolve())
    print(args.out.resolve())


if __name__ == "__main__":
    main()
