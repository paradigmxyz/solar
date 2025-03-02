# solar-ast

Solidity and Yul AST (Abstract Syntax Tree) type and visitor trait definitions.

## Overview

The `solar-ast` crate provides the Abstract Syntax Tree (AST) data structures and visitor patterns for Solidity and Yul code. It serves as a foundational component in the Solar compiler, representing the parsed structure of source code that can be traversed and analyzed by other compiler components.

## Features

- Complete AST node definitions for Solidity language constructs
- AST node definitions for Yul inline assembly
- Visitor trait implementations for traversing and transforming the AST
- Span tracking for precise source code location information
- Serialization support for AST nodes

## Usage

Add this crate to your dependencies:

```toml
[dependencies]
solar-ast = "0.1.0"
```

### Example: Working with AST nodes

```rust
use solar_ast::{ast::*, visit::*};

// Create a visitor implementation
struct MyVisitor;

impl<'ast> Visit<'ast> for MyVisitor {
    // Implement visitor methods for nodes you're interested in
    fn visit_contract_definition(&mut self, contract: &'ast ContractDefinition) {
        // Process contract definition
        println!("Found contract: {}", contract.name);
        
        // Continue traversing the AST
        walk_contract_definition(self, contract);
    }
}

// Use the visitor with an AST
fn process_ast(ast: &SourceUnit) {
    let mut visitor = MyVisitor;
    visitor.visit_source_unit(ast);
}
```

## Internal Structure

The crate is organized into several modules:

- `ast`: Core AST node definitions
- `visit`: Visitor trait and traversal functions
- `span`: Source location tracking
- `token`: Token definitions and utilities

## Related Crates

- `solar-parser`: Responsible for parsing Solidity code into AST structures
- `solar-sema`: Performs semantic analysis on the AST
