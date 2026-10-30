# solar-mir

MIR, the mid-level intermediate representation of the Solar compiler.

MIR is a typed SSA IR that sits between HIR and EVM IR. This crate holds:

- the module, function, block, instruction, and terminator types, the operation schema every
  instruction derives from, and the builder that constructs them;
- the text format's parser and printer;
- the verifier, with the CFG and call graph facts it needs.

The MIR passes, the lowering from HIR, and the backend that compiles MIR to EVM IR live in
`solar-codegen`.
