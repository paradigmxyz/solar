# Divergence from solc

This document explicitly lists intentional design decisions and differences between Solar and the official Solidity compiler (`solc`). Understanding these differences is important for users migrating tooling from `solc` to Solar.

## Overview

Solar aims to be compatible with `solc` for standard Solidity code, but makes intentional deviations in certain areas for performance, correctness, or implementation simplicity.

## Lexer & Literals

### Integer Literal Limits

**Behavior**: Solar rejects integer literals greater than 2^256.

**Reason**: Arbitrary-precision integers in the AST would cause significant performance degradation for marginal use cases. Solar uses fixed-size representations for better memory efficiency and faster parsing.

### Binary and Octal Literals

**Behavior**: Solar does not support binary (`0b...`) or octal (`0o...`) integer literals.

**Reason**: These are not part of the official Solidity specification. While `solc` may accept some of these in certain contexts, they are not portable and Solar rejects them with an error.

### Rational Numbers in Non-Decimal Bases

**Behavior**: Solar does not support rational numbers in binary, octal, or hexadecimal formats.

**Reason**: These are edge cases not well-specified in the language.

## Parser

### Yul Identifiers with Periods

**Behavior**: Solar rejects periods (`.`) in Yul identifiers.

**Reason**: Periods are not valid in the Solidity grammar for identifiers. While `solc` allows this in certain Yul contexts, Solar enforces the grammar more strictly.

### Quoted Pragma Names

**Behavior**: Solar accepts quoted pragma names (e.g., `pragma "abicoder" v1;`).

**Reason**: This is more permissive than `solc`, which does not accept quoted pragma names. Solar allows this for flexibility.

## Language Features

### Experimental Features

**Behavior**: Solar does not implement experimental Solidity features (e.g., `pragma experimental solidity;`).

**Reason**: Experimental features are unstable and rarely used. Solar focuses on the stable language specification.

### Unicode Direction Overrides

**Behavior**: Solar does not currently implement unicode direction override checks.

**Reason**: This is a low-priority security feature that will be implemented in a future release.

### NatSpec Documentation

**Behavior**: NatSpec documentation parsing and validation is not fully implemented.

**Reason**: This is a documentation feature that does not affect compiled output. It will be implemented when LSP features are completed.

## Type System

### Integer Literals in Some Contexts

**Behavior**: Solar may reject integer literals in some `lvalue` and base argument contexts where `solc` would succeed.

**Reason**: Full implicit conversion support for integer literals is being implemented in the typechecker. See [Tracking: Implicit conversions](https://github.com/paradigmxyz/solar/issues/617).

### Mapping Key Location Coercion

**Behavior**: Solar does not yet support location coercion for mapping keys (e.g., passing a `string memory` argument when the mapping expects a `string` key).

**Reason**: This requires additional type checking infrastructure that is currently being implemented.

## Interface and ABI

### ERC-165 Interface ID

**Behavior**: Solar follows `solc` by excluding inherited functions from interface ID calculations.

**Note**: This is documented for clarity as it is a point of alignment, not divergence.

### Referenced Events in ABI

**Behavior**: Solar's ABI output currently only includes events defined in a contract, not events referenced (emitted) by the contract.

**Reason**: This requires a resolved call graph from type checking. See [#305](https://github.com/paradigmxyz/solar/issues/305).

## Error Messages

Solar strives to match `solc`'s error codes where applicable, but may provide:
- More detailed error messages
- Better span highlighting
- Additional notes and suggestions

These are considered improvements rather than divergences.

## Future Compatibility

As Solar continues development toward full `solc` compatibility, many of these divergences (particularly in the type system) will be resolved. Items marked with issue links are actively being worked on.

If you encounter a divergence not listed here that affects your workflow, please [open an issue](https://github.com/paradigmxyz/solar/issues/new).
