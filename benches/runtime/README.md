# Codegen benchmark corpus

This directory contains fixtures and workload documentation for the codegen benchmark. The shared
project archives live in `../../testdata/projects/`; archives group cases from the same upstream
project. The default `runtime` mode selects each entrypoint's transitive Solidity import closure and
omits the heavy full-project cases. The `compile-time` mode measures those cases by passing full
archived Standard JSON inputs to both compilers without deployment or runtime workloads. CI runs
both modes with `--mode runtime compile-time`.

Keeping the inputs here makes the benchmark reproducible from this checkout and removes the CI
dependency on a second repository and its recursive submodules.

Pass `--evm-version VERSION` to replace every archived Standard JSON target and benchmark a whole
corpus against one EVM version. Use `--solar-only` when the selected target is not supported by the
installed solc. When available, solc still provides helper contracts for cold-path runtime checks.

Use `--solar-only` for repeated local runs after recording a two-compiler baseline. This skips the
reference solc compile for each case while retaining Solar compilation, gas measurements, and
runtime failure checks. A one-compiler run cannot make differential runtime claims, so successful
runtime comparisons are marked as skipped unless a matching reference result is supplied.

Pass `--reference-results PATH` with `--solar-only` to reuse matching solc results from a prior
run. The benchmark copies solc compile, gas, and runtime data only when the input fingerprint
matches, then performs the normal cross-compiler runtime checks. PR CI uses the exact-base result
as the reference, so solc runs on the base revision instead of repeating unchanged work on the PR.

Pass `--artifacts PATH` to write a file tree for each runtime case and compiler. This extra compile
runs outside the timed samples. Solar emits MIR, creation and runtime EVM IR, disassembly, bytecode,
and raw Standard JSON input and output. Solc emits optimized Yul IR where available, disassembly,
bytecode, and raw Standard JSON input and output. When `--reference-results` points to a result next
to an `artifacts` directory, the matching solc files are copied into the new run.

The workload definitions and helper fixtures were imported from
[`walnuthq/solidity-compiler-benchmarks`](https://github.com/walnuthq/solidity-compiler-benchmarks)
at commit `01209d2b8ac81645b92e3ef801b5bcdfd61bfd69`. The combined profile still contains each contract
from both compilers, both deployed artifacts, the same ordered transactions, and matching normalized
runtime observations.

| Case | Upstream source | Revision | Files |
| --- | --- | --- | ---: |
| `uniswap-v2-pair` | `Uniswap/v2-core` | `ee547b17853e71ed4e0101ccfd52e70d5acded58` | 10 |
| `openzeppelin-erc20-mock` | `OpenZeppelin/openzeppelin-contracts` | `openzeppelin-5.6.1` archive | 6 |
| `openzeppelin-vesting-wallet` | `OpenZeppelin/openzeppelin-contracts` | `openzeppelin-5.6.1` archive | 12 |
| `nitro-one-step-proof` | `OffchainLabs/nitro-contracts` | `0b8c04e8f5f66fe6678a4f53aa15f23da417260e` | 22 |
| `aave-l2-encoder` | `aave/aave-v3-core` | `782f51917056a53a2c228701058a6c3fb233684a` | 6 |
| `lilweb3-ens` | `m1guelpf/lil-web3` | `7346bd28c2586da3b07102d5290175a276949b15` | 1 |
| `lilweb3-flashloan` | `m1guelpf/lil-web3` plus `transmissions11/solmate` | `7346bd28c2586da3b07102d5290175a276949b15`, `e802bcf2fb24dda2bf7e513bea86d15c48b57486` | 2 |
| `lilweb3-fractional` | `m1guelpf/lil-web3` plus `transmissions11/solmate` | `7346bd28c2586da3b07102d5290175a276949b15`, `e802bcf2fb24dda2bf7e513bea86d15c48b57486` | 3 |
| `maple-erc20` | `maple-labs/erc20` | `baf791a9f894b0b319a2d42d5b9f8d30349ebaad` | 2 |

The OpenZeppelin cases share the canonical
`../../testdata/projects/openzeppelin-5.6.1.json.gz` archive; the file counts above are the sliced
closure for each case. This replaces the extracted OpenZeppelin
runtime archive used by earlier versions of the benchmark.

The Lil Web3 cases share `../../testdata/projects/lilweb3-runtime.json.gz`; the file counts above
are the sliced closure for each case.

The large OpenZeppelin and Solady cases use the pinned archives in `../../testdata/projects/`. The
normal benchmark suite also reads those archives from there.

The three additional micro contracts (`../../testdata/Arithmetic.sol`,
`../../testdata/Factorial.sol`, and `../../testdata/SumArray.sol`) came from the benchmark repository at the commit above. The
runtime suite reuses the existing `../../testdata/Counter.sol` source from the normal benchmark
suite. The Aave harness is embedded in `../../testdata/projects/aave-l2-encoder.json.gz`.
`fixtures/runtime/RuntimeFixtures.sol` provides local Apache-2.0 helpers with the same interfaces
used by the cold-path workloads. Embedded Solidity sources retain their SPDX identifiers.
