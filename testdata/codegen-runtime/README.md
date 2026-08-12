# Codegen runtime corpus

This directory contains the self-contained inputs used by the codegen runtime benchmark. The
repository cases are stored as compressed standard-json inputs under `projects/`; archives group
cases from the same upstream project. The runner selects only each entrypoint's transitive
Solidity import closure before compiling it. Keeping the inputs here makes the benchmark
reproducible from this checkout and removes the CI dependency on a second repository and its
recursive submodules.

The workload definitions and helper fixtures were imported from
[`walnuthq/solidity-compiler-benchmarks`](https://github.com/walnuthq/solidity-compiler-benchmarks)
at commit `01209d2b8ac81645b92e3ef801b5bcdfd61bfd69`. The benchmark still compiles each contract with
both compilers, deploys both artifacts, executes the same ordered transactions, and requires their
normalized runtime observations to match.

| Case | Upstream source | Commit | Files |
| --- | --- | --- | ---: |
| `uniswap-v2-pair` | `Uniswap/v2-core` | `ee547b17853e71ed4e0101ccfd52e70d5acded58` | 10 |
| `openzeppelin-erc20-mock` | `OpenZeppelin/openzeppelin-contracts` | `6308fdc5e8e0d5e8a94dc9d5d4c79f6331334c81` | 6 |
| `openzeppelin-vesting-wallet` | `OpenZeppelin/openzeppelin-contracts` | `6308fdc5e8e0d5e8a94dc9d5d4c79f6331334c81` | 12 |
| `nitro-one-step-proof` | `OffchainLabs/nitro-contracts` | `0b8c04e8f5f66fe6678a4f53aa15f23da417260e` | 22 |
| `aave-l2-encoder` | `aave/aave-v3-core` | `782f51917056a53a2c228701058a6c3fb233684a` | 6 |
| `lilweb3-ens` | `m1guelpf/lil-web3` | `7346bd28c2586da3b07102d5290175a276949b15` | 1 |
| `lilweb3-flashloan` | `m1guelpf/lil-web3` plus `transmissions11/solmate` | `7346bd28c2586da3b07102d5290175a276949b15`, `e802bcf2fb24dda2bf7e513bea86d15c48b57486` | 2 |
| `lilweb3-fractional` | `m1guelpf/lil-web3` plus `transmissions11/solmate` | `7346bd28c2586da3b07102d5290175a276949b15`, `e802bcf2fb24dda2bf7e513bea86d15c48b57486` | 3 |
| `maple-erc20` | `maple-labs/erc20` | `baf791a9f894b0b319a2d42d5b9f8d30349ebaad` | 2 |

The OpenZeppelin cases share `projects/openzeppelin-runtime.json.gz`, and the Lil Web3 cases
share `projects/lilweb3-runtime.json.gz`; the file counts above are the sliced closure for each
case.

The large OpenZeppelin and Solady cases use the pinned archives in this same `projects/`
directory. The normal benchmark suite also reads those archives from here.

The three additional micro contracts (`../Arithmetic.sol`, `../Factorial.sol`, and
`../SumArray.sol`) came from the benchmark repository at the commit above. The runtime suite
reuses the existing `../Counter.sol` source from the normal benchmark suite. The Aave harness is
embedded in `projects/aave-l2-encoder.json.gz`.
`fixtures/runtime/RuntimeFixtures.sol` provides local Apache-2.0 helpers with the same interfaces
used by the cold-path workloads. Embedded Solidity sources retain their SPDX identifiers.
