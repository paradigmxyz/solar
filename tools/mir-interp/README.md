# solar-mir-interp

Runs lowered MIR in the compiler's MIR interpreter, which the UI test runner checks `run-call`
directives against. The library side is `solar_codegen::interpret`.

The input is one lowered MIR module, such as a fixture under `tests/ui/codegen/mir/`, or the output
of `solar -Zdump=mir-final`, which prints the final MIR of each contract. The tool runs only a module the validator accepts, as the backend
compiles only such modules. `-` reads standard input, so a contract can go straight from the
compiler to the interpreter:

```bash
solar -Zdump=mir-final Token.sol | cargo run -p solar-mir-interp -- - --call 'totalSupply() returns (uint256)'
```

## Transactions

Without `--function`, the tool runs a transaction on the module's dispatch entry, from empty
memory, with the calldata that `--calldata` gives in hex or that `--call` encodes from a signature
and arguments:

```bash
cargo run -p solar-mir-interp -- token.mir --contract Token \
    --call 'transfer(address,uint256)' 0x3C44CdDdB6a900fa2b585dd299e03d12FA4293BC 125 \
    --context caller=0x70997970c51812dc3a010c7d01b50e0d17dc79c8 \
    --storage 0x14e04a66bf74771820a7400ff6cf065175b3d7eb25805a5bd1633b161af5d101=1000
```

A signature that ends in `returns (...)`, such as `balanceOf(address) returns (uint256)`, also
decodes what the call returns, and `Error(string)` and `Panic(uint256)` reverts are decoded
whatever the signature. `--contract` picks a module of a dump that holds several, by its full
name or its contract name.

## Functions

`--function NAME` runs one internal function on the words `--arg` gives, in decimal or `0x` hex,
from memory that holds only the free memory pointer. External entries read their arguments from
calldata, so they run in transactions instead.

```bash
cargo run -p solar-mir-interp -- tests/ui/codegen/mir/interp/calls.mir --function double --arg 21
```

## Storage and context

Storage reads return what `--storage SLOT=VALUE` sets and zero otherwise; context reads return what
`--context NAME=VALUE` sets, such as `caller`, `callvalue`, `timestamp`, or `chainid`, and zero
otherwise. Each run starts from that state, so replaying a sequence of transactions means passing
the storage one run leaves to the next. `--heap-start` sets where the free memory pointer starts,
`0x80` unless the backend placed frames above it; the compiled bytecode stores it at `0x40` first.

## Output

The tool prints how the run ended, the events it logged, and the persistent and transient storage
it wrote; `--json` prints the same as JSON. `--trace` prints every operation to standard error as it
runs, indented by call depth, with its operand values and result:

```text
@add bb0: v0 = add arg0, arg1  [arg0 = 0x2, arg1 = 0x3] -> 0x5
@add bb0: v1 = lt v0, arg0  [v0 = 0x5, arg0 = 0x2] -> 0x0
@add bb0: jumpi v1, bb1, bb2  [v1 = 0x0]
@add bb2: ret v0  [v0 = 0x5]
```

The exit code is 0 when the run ends, however it ends, 1 on errors, and 2 when the interpreter
cannot run what the code reaches: calls to other contracts, contract creation, and `gas`.
