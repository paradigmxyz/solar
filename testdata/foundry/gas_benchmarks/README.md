# Gas Benchmarks

This directory contains contracts specifically designed to measure and benchmark gas usage between Solar and solc.

## Benchmark Categories

### 1. Stack Operations (`StackBench.sol`)
- DUP/SWAP minimization
- Dead value elimination
- Stack depth management

### 2. Memory Operations (`MemoryBench.sol`)
- MLOAD/MSTORE patterns
- Memory expansion costs
- ABI encoding overhead

### 3. Storage Operations (`StorageBench.sol`)
- SLOAD/SSTORE batching
- Storage packing
- Warm vs cold access patterns

### 4. Arithmetic (`ArithmeticBench.sol`)
- Constant folding opportunities
- Identity operation elimination
- Complex expression optimization

### 5. Control Flow (`ControlFlowBench.sol`)
- Branch optimization
- Loop optimization
- Function inlining candidates

### 6. Common Subexpression (`CSEBench.sol`)
- Repeated computation patterns
- Expression caching opportunities

## Running Benchmarks

```bash
# From the Solar root directory
cargo test -p solar-codegen --test foundry -- gas_benchmark --nocapture
```

## Expected Outcomes

Solar should produce:
- **Smaller bytecode** (65-85% smaller typically)
- **Equal or lower gas** for all operations
- **Faster compilation** for large projects

## Adding New Benchmarks

1. Create a new `.sol` file in this directory
2. Add corresponding test file in `test/`
3. Update the foundry harness if needed
