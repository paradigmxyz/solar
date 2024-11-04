# solar-parse

Solidity and Yul lexer and parser.

The implementation of both are modified from [`rustc_lexer`] and [`rustc_parse`] in the Rust
compiler to accommodate the differences between Rust and Solidity/Yul.

[`rustc_lexer`]: https://github.com/rust-lang/rust/blob/a2a1206811d864df2bb61b2fc27ddc45a3589424/compiler/rustc_lexer/src/lib.rs
[`rustc_parse`]: https://github.com/rust-lang/rust/blob/a2a1206811d864df2bb61b2fc27ddc45a3589424/compiler/rustc_parse/src/lib.rs
