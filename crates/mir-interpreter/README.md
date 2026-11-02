# solar-mir-interpreter

Runs lowered MIR, the mid-level intermediate representation of the Solar compiler (see
`solar-mir`), as the EVM runs the bytecode the compiler generates from it.

`transact` runs a transaction: a module's dispatch entry on calldata, with zeroed memory and a
`Host` answering what the contract reads from its context, such as the caller or a storage slot's
value before the transaction. It takes the text of one lowered module, as `solar -Zdump=mir-final`
prints it, and `parse_dump` splits such a dump into its modules.

```rust,ignore
use alloy_primitives::U256;
use solar_mir_interpreter::{Host, Options, Outcome, transact};

struct Empty;

impl Host for Empty {
    fn read(&mut self, _opcode: u8, _operands: &[U256]) -> Option<U256> {
        Some(U256::ZERO)
    }

    fn storage(&mut self, _slot: U256) -> U256 {
        U256::ZERO
    }
}

let mir = "@module m\n@phase lowered\nfn @entry() [entry] {\n  bb0:\n    v0 = add 2, 3\n    mstore 0, v0\n    returndata 0, 32\n}\n";
let execution = transact(mir, &[], &mut Empty, Options::default()).unwrap();
assert_eq!(execution.outcome, Outcome::Success(U256::from(5).to_be_bytes::<32>().to_vec()));
```

Each instruction runs by the semantics its operation schema row declares, so the interpreter lists
no operations of its own. The schema names opcodes by mnemonic, which the interpreter looks up in
the opcode table of `solar-evmir`; pure opcodes compute with the table's word semantics, as the
compiler's constant folding and the EVM IR interpreter do. Memory, storage, transient storage,
events, calldata, and return data are the EVM's. A final dump also records the memory placements
the backend chose, such as deferred allocations, multi-return buffers, heap starts, and the end of
its frames, so pointers and `msize` match the bytecode. The interpreter models one contract: calls,
contract creation, and `gas` end a run as unsupported, and so does a module the compiler dumped
before lowering it, as `-O none -Zdump=mir` does.

The compiler's UI test runner executes every `run-call` directive in an EVM. In a test written in
lowered MIR, it also runs the call through `transact` on the MIR the compiler lowered to that
bytecode. Both execute the same compiled program, so a disagreement is a bug in the backend or in
the interpreter.
