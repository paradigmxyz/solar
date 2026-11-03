//! Compiles the EVM opcode schema in `ops/` into Rust.

fn main() {
    solar_op_dsl::build(solar_op_dsl::Dialect::Evm, "ops");
}
