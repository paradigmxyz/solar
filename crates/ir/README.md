# solar-ir

Shared building blocks of the compiler's IRs, which MIR and EVM IR both use:

- `link`: program data, library identities, and relocatable bytecode;
- `debug`: the bounds of source-origin metadata;
- `ir_parse`: the lexer-level parser of the IR text formats.

The IRs themselves live in `solar-mir` and `solar-evmir`.
