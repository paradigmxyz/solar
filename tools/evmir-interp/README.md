# solar-evmir-interp

Runs EVM IR in the compiler's EVM IR interpreter, which the UI test runner checks `run-call`
directives against. The library side is the `solar-evmir-interpreter` crate.

The input is one EVM IR module, such as a fixture under `tests/ui/codegen/evm-ir/`, or the output
of `solar -Zdump=evm-ir-runtime-scheduled`, which prints the runtime EVM IR of each contract as
stack scheduling emits it, before the EVM IR pipeline. `-Zdump=evm-ir-runtime` prints the same
program after the pipeline, which runs too. `-` reads standard input, so a contract can go straight
from the compiler to the interpreter:

```bash
solar -Zdump=evm-ir-runtime-scheduled Token.sol | cargo run -p solar-evmir-interp -- - --call 'totalSupply() returns (uint256)'
```

## Transactions

The tool runs a transaction on the program from its first block, with an empty stack, zeroed
memory, and the calldata that `--calldata` gives in hex or that `--call` encodes from a signature
and arguments:

```bash
cargo run -p solar-evmir-interp -- token.dump --contract Token \
    --call 'transfer(address,uint256)' 0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC 125 \
    --context caller=0x70997970c51812dc3a010c7d01b50e0d17dc79c8 \
    --storage 0x14e04a66bf74771820a7400ff6cf065175b3d7eb25805a5bd1633b161af5d101=1000
```

A signature that ends in `returns (...)`, such as `balanceOf(address) returns (uint256)`, also
decodes what the call returns, and `Error(string)` and `Panic(uint256)` reverts are decoded
whatever the signature. `--contract` picks a module of a dump that holds several: by its header, as
in `src/Token.sol:Token (scheduled runtime)`, by its contract's full name, or by the contract's
name.

## Storage and context

Storage reads return what `--storage SLOT=VALUE` sets and zero otherwise; context reads return what
`--context NAME=VALUE` sets, such as `caller`, `callvalue`, `timestamp`, or `chainid`, and zero
otherwise. Each run starts from that state, so replaying a sequence of transactions means passing
the storage one run leaves to the next. `--evm-version` sets the target, which decides the opcodes
the program may use.

## Code layout

The assembler decides where blocks and data go, so the interpreter places them itself, and `codesize`
and pushed block addresses report its own choices. It runs every transaction a second time with
them moved, and reports a run whose result changes with them as unsupported, since the bytecode
would see other addresses.

## Output

The tool prints how the run ended, the events it logged, and the persistent and transient storage
it wrote; `--json` prints the same as JSON. `--trace` prints every operation to standard error as it
runs, with the words it read, top of the stack first, and the word it pushed:

```text
bb0: push 2 -> 0x2
bb0: push 3 -> 0x3
bb0: add  [0x3, 0x2] -> 0x5
bb0: jumpi bb1, bb2  [0x5]
```

The exit code is 0 when the run ends, however it ends, 1 on errors, and 2 when the interpreter
cannot run what the code reaches: calls to other contracts, contract creation, `gas`, immutables,
library addresses, and results that depend on the code layout.
