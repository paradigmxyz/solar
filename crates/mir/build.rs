//! Compiles the MIR operation schema in `ops/` into Rust.

fn main() {
    solar_op_dsl::build(solar_op_dsl::Dialect::Mir, "ops");
}
