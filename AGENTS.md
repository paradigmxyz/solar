# AGENTS.md

Guidance for AI coding agents working in this repository.

## Project Overview

Solar is a blazingly fast, modular Solidity compiler written in Rust, aiming to be a modern alternative to solc.

## Commands

```bash
cargo build                                      # Build
cargo nextest run --workspace                    # Run tests (faster than cargo test)
cargo uitest                                     # Run UI tests
cargo uibless                                    # Update UI test expectations
cargo +nightly fmt --all                         # Format (CI uses nightly)
cargo clippy --workspace --all-targets           # Lint
cargo run -- file.sol                            # Run compiler
cargo run -- -Zhelp                              # Unstable flags help
```

## Architecture

- **solar-parse**: Lexer and parser
- **solar-ast**: AST definitions and visitors
- **solar-sema**: Semantic analysis (symbol resolution, type checking)
- **solar-interface**: Diagnostics and source management
- **solar-cli**: Command-line interface

Pipeline: Lexing → Parsing → Semantic Analysis → (IR → Codegen, planned)

### Visitor Pattern

Use `type BreakValue = Never` if visitor never breaks. Override `visit_*` methods and always call `walk_*` to continue traversal:

```rust
fn visit_expr(&mut self, expr: &'ast Expr) -> ControlFlow<Self::BreakValue> {
    // Your logic here
    walk_expr(self, expr)  // Always use walk_* for child traversal
}
```

## Testing

- **Unit tests**: In source files
- **UI tests**: In `tests/ui/`, verify compiler output
- Auxiliary files go in `auxiliary/` subdirectory

### UI Test Annotations

```solidity
//@compile-flags: --emit=abi
contract Test {
    uint x; //~ ERROR: message here
    //~^ NOTE: note about previous line
}
```

Annotations: `//~ ERROR:`, `//~ WARN:`, `//~ NOTE:`, `//~ HELP:`
Use `^` or `v` to point to lines above/below.

## Notes

- **Symbol comparisons**: Use `sym::name` or `kw::Keyword` instead of `.as_str()` for performance. Add new symbols to `crates/macros/src/symbols.rs`.
- **Arena allocation**: AST nodes use arenas for performance.
- **Benchmarks**: See @benches/README.md to benchmark when working on performance-critical code.
