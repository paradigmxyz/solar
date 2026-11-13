# Uniswap V2 target

The target of the solar-tv and solar-verify Uniswap V2 work: the contracts of
[Uniswap/v2-core](https://github.com/Uniswap/v2-core) at
6a9e7c97860676e0992f22a49665760444c1cdf5, written for Solidity 0.5.16, ported to ^0.8 with
the fewest changes that compile and keep their meaning. v2-core is GPL-3.0, so this directory
keeps the port as edits, not as code: [port.py](port.py) clones v2-core at that commit and
writes the port, with v2-core's LICENSE next to it.

```sh
python3 solar-tv/uniswap-v2/port.py --out target/uniswap-v2/src   # the default --out
python3 solar-tv/uniswap-v2/coverage.py                           # writes COVERAGE.md
```

The port holds `UniswapV2Pair`, `UniswapV2ERC20`, `UniswapV2Factory`, the libraries `Math`,
`SafeMath` and `UQ112x112`, and the interfaces they import, at the same paths as upstream's
`contracts/`. Upstream's test contracts are left out. [coverage.py](coverage.py) measures how far
the translation validation and the verifier get on it; its output is [COVERAGE.md](COVERAGE.md).

## Differences from 0.5.16

Every change port.py makes, and nothing else; each edit must match exactly once:

- Every `pragma solidity =0.5.16;` and `pragma solidity >=0.5.0;` is `pragma solidity ^0.8.0;`.
- Constructors lose `public`, which ^0.8 rejects: `UniswapV2ERC20`'s, `UniswapV2Pair`'s and
  `UniswapV2Factory`'s.
- `uint(-1)` is `type(uint).max` (`UniswapV2ERC20.transferFrom`'s infinite allowance) and
  `uint112(-1)` is `type(uint112).max` (`UniswapV2Pair._update`'s reserve bound): ^0.8 rejects
  converting a negative literal, and these are the values 0.5.16 computed.
- In `UniswapV2ERC20`'s constructor the inline assembly `chainid` is `chainid()`, the call syntax
  ^0.8's Yul requires; it reads the same value.
- `UniswapV2Pair` inherits `UniswapV2ERC20` only, not also `IUniswapV2Pair`: ^0.8 makes a function
  two bases declare need an `override(...)` naming both, which every ERC20 function of the pair
  would need. The interface has no code, so the pair's ABI and code are the same.
- `UniswapV2ERC20` and `UniswapV2Factory` no longer declare the events their interfaces
  (`IUniswapV2ERC20`, `IUniswapV2Factory`) declare identically: ^0.8 rejects an event declared
  twice in one contract's hierarchy. They emit the same events, from the interface's declaration.
- In `UniswapV2Pair._update`, the time elapsed since the last update and the two price
  accumulators are in an `unchecked` block. Upstream relies on them wrapping ("overflow is
  desired"): the 32-bit timestamp wraps every 136 years, and the accumulators wrap modulo 2^256,
  which their consumers difference. ^0.8 would revert there instead.
- `SafeMath`'s three bodies are `unchecked`: each computes the wrapped result and checks it
  itself, as in 0.5.16, so an overflow reverts with its `ds-math-*` reason rather than the
  compiler's `Panic(0x11)`.

All other arithmetic is checked under ^0.8 where 0.5.16 wrapped. None of it is meant to wrap:
`Math.sqrt`'s `y / x + x`, `UniswapV2ERC20.permit`'s `nonces[owner]++` and
`UQ112x112.encode`'s product stay in range for every input the pair passes them, so the port
reverts on exactly the inputs where 0.5.16 would have computed a wrapped value no caller reaches.

## Certifying the factory

```sh
solar-tv/uniswap-v2/certify-factory.sh   # port.py, flatten, milestone/certify.sh
```

[certify-factory.sh](certify-factory.sh) writes the port, flattens the factory's unit into
`target/uniswap-v2/factory.sol` and runs [certify.sh](../milestone/certify.sh) on
`UniswapV2Factory` as `uni_factory`. The generated stages and proofs land in
`solar-tv/milestone/uni_factory`, which `.gitignore` keeps out of the repository with the
GPL source they come from. The factory's runtime has no loops; it embeds the pair's creation
code as program data, which the assembly walk leaves after the code it covers. All 18 stage
proofs, their composition from the first lowered MIR to the final EVM IR, and the assembly walk
check, with only `propext`, `Classical.choice` and `Quot.sound`. They need more of Lean
than the other milestone contracts: the heartbeat bound is lifted (`CERTIFY_HEARTBEATS=0`;
the kernel still checks every proof), each EVM IR run takes about 25 minutes and up to 7 GB,
and the kernel needs more than 12 GB for stack scheduling's proof (67 minutes), so the script
runs Lean with `LEAN_MEMORY=13800` by default.
