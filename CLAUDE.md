# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Solar is a blazingly fast, modular Solidity compiler written in Rust. It aims to be a modern alternative to solc with focus on performance, modularity, and developer experience.

## Common Development Commands

### Building and Testing
```bash
# Build the project
cargo build

# Run tests (recommended - faster)
cargo nextest run --workspace

# Run specific test
cargo nextest run --workspace test_name

# Run UI tests (compiler behavior tests)
cargo uitest

# Update UI test expectations after changes
cargo uibless
```

### Code Quality
```bash
# Format code (CI uses nightly)
cargo +nightly fmt --all

# Run clippy lints (as CI does)
RUSTFLAGS="-D warnings" cargo clippy --workspace --all-targets

# Check for typos
typos
```

### Benchmarking
```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench benchmark_name
```

## Architecture

Solar is organized as a multi-crate workspace with clear separation of concerns:

- **solar**: Main compiler library and binary
- **solar-ast**: Abstract syntax tree definitions and visitors
- **solar-parse**: Lexer and parser implementation
- **solar-sema**: Semantic analysis (symbol resolution, type checking)
- **solar-interface**: Compiler interface, diagnostics, and source management
- **solar-cli**: Command-line interface
- **solar-config**: Configuration management
- **solar-data-structures**: Core data structures (arena, indexmap, etc.)
- **solar-macros**: Procedural macros for AST and other derivations

### Compilation Pipeline

1. **Lexing** (solar-parse): Tokenizes Solidity source code
2. **Parsing** (solar-parse): Builds AST from tokens
3. **Semantic Analysis** (solar-sema): 
   - Symbol resolution (implemented)
   - Type checking (in progress)
   - Static analysis (planned)
4. **Middle-end** (planned): IR generation and optimizations
5. **Back-end** (planned): Code generation

### Key Design Patterns

- **Arena allocation**: AST nodes are allocated in arenas for performance
- **Visitor pattern**: Used extensively for AST traversal (see `solar-ast/src/visit.rs`)
- **Diagnostic system**: Structured error reporting with source locations
- **Session-based compilation**: All compilation state managed through `Session`

## Testing Strategy

- **Unit tests**: Standard Rust tests in source files
- **UI tests**: Integration tests that verify compiler output (tests/ui/)
- **Solidity test suite**: Compatibility testing against official Solidity tests (testdata/solidity/)

When adding new features or fixing bugs:
1. Add unit tests for internal logic
2. Add UI tests for user-visible behavior
3. Run `cargo uibless` if UI test output changes are expected

### Test File Organization

When creating test files that require auxiliary files (e.g., imports, helper contracts):
- Place auxiliary test files in an `auxiliary/` directory within the current test directory
- The test runner will automatically ignore files in `auxiliary/` directories
- This keeps test organization clean and prevents auxiliary files from being run as tests themselves

Example:
```
tests/ui/imports/
├── unused_imports.sol      # Main test file
└── auxiliary/             # Helper files for the test
    ├── Library.sol
    ├── Library2.sol
    └── Helpers.sol
```

## Performance Considerations

Solar prioritizes performance through:
- Zero-copy parsing where possible
- Arena allocation for AST nodes
- String interning for identifiers
- Efficient data structures (e.g., `IndexMap` for deterministic iteration)

When working on performance-critical code:
1. Use existing benchmarks or add new ones
2. Measure before and after changes with `cargo bench`
3. Consider memory allocation patterns