# solar-evmir

EVM IR, the machine-level intermediate representation of the Solar compiler's EVM backend, and the
EVM knowledge it rests on.

EVM IR sits between stack scheduling and the assembler: basic blocks of opcodes with explicit
`dup`, `swap`, and `pop` operations, explicit terminators, and program data, before code addresses
exist. This crate holds:

- `ir`: the module, block, instruction, and terminator types, the text format's parser and
  printer, and the verifier;
- `op`: the opcode table, with each opcode's stack effect, traits, gas tier, and availability
  across EVM versions, and the word semantics of the pure opcodes;
- `gas`: the gas schedule of each opcode tier across EVM versions.

`link`, `debug`, and `ir_parse`, which MIR shares, come from `solar-ir` and are re-exported here.

The compiler's EVM IR passes and the assembler live in `solar-codegen`, which compiles EVM IR to
bytecode.

```rust,ignore
use solar_evmir::ir::Module;
use solar_interface::{Session, source_map::FileName};

let sess = Session::builder().with_stderr_emitter().build();
sess.enter_sequential(|| {
    let file = sess
        .source_map()
        .new_source_file(FileName::Custom("program.evmir".into()), "@module m\nbb0:\n  stop\n")
        .unwrap();
    let module = Module::parse(&sess, &file).unwrap();
    print!("{}", module.to_text());
});
```
