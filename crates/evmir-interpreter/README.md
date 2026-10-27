# solar-evmir-interpreter

Runs EVM IR, the machine-level intermediate representation of the Solar compiler (see
`solar-evmir`), as the EVM runs the bytecode the program assembles to.

`transact` runs a transaction on the program of one EVM IR module: it enters the program's first
block with the transaction's calldata, an empty stack, and zeroed memory, and asks a `Host` for
what the program reads from its context, such as the caller or a storage slot's value before the
transaction. It takes the module as text: a `.evmir` file, or a module of the runtime EVM IR the
compiler dumps, as scheduled with `solar -Zdump=evm-ir-runtime-scheduled` or as assembled with
`-Zdump=evm-ir-runtime`. `parse_dump` splits such a dump into its modules.

```rust,ignore
use alloy_primitives::U256;
use solar_evmir_interpreter::{Host, Options, Outcome, transact};

struct Empty;

impl Host for Empty {
    fn read(&mut self, _opcode: u8, _operands: &[U256]) -> Option<U256> {
        Some(U256::ZERO)
    }

    fn storage(&mut self, _slot: U256) -> U256 {
        U256::ZERO
    }
}

let program = "@module m\nbb0:\n  push 2\n  push 3\n  add\n  push 0\n  mstore\n  push 32\n  push 0\n  return\n";
let execution = transact(program, &[], &mut Empty, Options::default()).unwrap();
assert_eq!(execution.outcome, Outcome::Success(U256::from(5).to_be_bytes::<32>().to_vec()));
```

The stack, memory, storage, transient storage, events, and calldata are the EVM's, and pure
opcodes evaluate through the same word semantics as the compiler's constant folding. The
interpreter models one contract: calls, contract creation, `gas`, immutables, and library
addresses end a run as unsupported. EVM IR leaves code addresses to the assembler, so the
interpreter places blocks and data itself, and runs each transaction again with them moved: a
result that changes with them is unsupported, since the bytecode sees other addresses.

`CODESIZE` remains unknown before assembly: two layouts can agree on a comparison whose result
differs at the assembled size. Its value may be discarded or used unmodified as `CODECOPY`'s source
offset, which reads zeros past the code's end. Other uses, including arithmetic and comparisons,
are unsupported. The two-layout check detects other address-dependent results but does not prove
arbitrary expressions independent of layout.

The compiler's UI test runner executes every `run-call` directive in an EVM. With
`SOLAR_RUN_CALL_EVM_IR` set, it also runs the call through `transact` on the runtime EVM IR the
compiler scheduled for that bytecode, before the EVM IR pipeline, and a test written in EVM IR
checks each call on its own text. The EVM runs that program after the pipeline and assembly, so a
disagreement is a bug in an EVM IR pass, the assembler, or the interpreter.
