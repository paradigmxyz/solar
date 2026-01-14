# Solar Codegen Tests

## Foundry Integration Tests

These tests compare Solar's EVM codegen against solc by:
1. Compiling the same Solidity source with both compilers
2. Deploying to a local anvil instance
3. Running the same operations and comparing results

### Prerequisites

- `anvil` (from Foundry)
- `cast` (from Foundry)
- `solc` (Solidity compiler)

### Running Tests

```bash
# Run all foundry integration tests (sequential to avoid port conflicts)
cargo test -p solar-codegen --test foundry -- --test-threads=1

# Run with output
cargo test -p solar-codegen --test foundry -- --test-threads=1 --nocapture

# Run a specific level
cargo test -p solar-codegen --test foundry test_level3 -- --test-threads=1 --nocapture
```

### Test Levels

| Level | Test | Description |
|-------|------|-------------|
| 0 | `test_level0_empty_contract` | Empty contract - just deploys |
| 1 | `test_level1_constant_return` | Pure function returning constant |
| 2 | `test_level2_storage_read` | Public state variable (auto-getter) |
| 3 | `test_level3_storage_write` | Increment counter with storage write |
| 4 | `test_level4_multiple_functions` | Multiple public functions |
| 5 | `test_level5_arithmetic` | Add, sub, mul, div operations |
| 6 | `test_level6_multiple_storage` | Multiple storage slots |
| 7 | `test_level7_conditionals` | If statements, comparisons |
| 8 | `test_level8_loops` | For and while loops |
| 9 | `test_level9_booleans` | Boolean type and operations |
| 10 | `test_level10_address` | Address type and msg.sender |
| 11 | `test_level11_arithmetic_runtime` | Verify arithmetic at runtime |
| 12 | `test_level12_conditional_runtime` | Verify conditionals at runtime |
| - | `test_summary_stats` | Print size comparison table |

### Current Results

```
=== Compilation Size Comparison ===
Contract               Solar         Solc  Reduction
----------------------------------------------------
Empty                    8 B         92 B        92%
Counter                 63 B        399 B        85%
Math                    41 B        502 B        92%
```

Solar produces 85-92% smaller bytecode by omitting:
- Memory pointer initialization
- Overflow/underflow checks
- Revert reason strings
- Contract metadata (CBOR)

### Known Limitations

Not yet implemented in Solar codegen:
- `new Contract()` - contract creation
- External calls
- Events (LOG opcodes)
- Complex types (arrays, mappings, structs)
- Inheritance
