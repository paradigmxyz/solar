# sulk-bench

Simple benchmarks across different Solidity parser implementations.

Run with:
```bash
# Criterion
cargo r -r --manifest-path benches/Cargo.toml -- --bench

# Valgrind
# See `--valgrind --help`
cargo b -r --manifest-path benches/Cargo.toml
valgrind --tool=callgrind benches/target/release/sulk-bench --valgrind
```

This crate is excluded from the main workspace to avoid compiling it (and its dependencies) when
invoking other commands such as `cargo test`.

Note that currently `OptimizorClub` must be patched because `slang` fails parsing it: <https://github.com/NomicFoundation/slang/issues/740>
```patch
diff --git a/test/benchmarks/OptimizorClub.sol b/test/benchmarks/OptimizorClub.sol
index c21d42e65f..b02faa06cc 100644
--- a/test/benchmarks/OptimizorClub.sol
+++ b/test/benchmarks/OptimizorClub.sol
@@ -69,7 +69,7 @@ library Puretea {
                 } lt(offset, end) {
                     offset := add(offset, 1)
                 } {
-                    let opcode := byte(0, mload(offset))
+                    // let opcode := byte(0, mload(offset))
                     if iszero(matchesMask(mask, opcode)) {
                         leave
                     }
```

## Results

For this compiler:
- ~3 µs to set and unset scoped-TLS; **not** included below.
- ~400 ns to setup and drop stderr emitter; included below.
- ~200 ns to setup and drop the parser; included below.

In practice all of these are one-time costs.

Criterion results on `x86_64-unknown-linux-gnu` on AMD Ryzen 7 7950X:

```
parser/empty/sulk       time:   [710.47 ns 711.88 ns 713.51 ns]
parser/empty/solang     time:   [122.28 ns 122.86 ns 123.32 ns]
parser/empty/slang      time:   [15.189 µs 15.210 µs 15.231 µs]

parser/simple/sulk      time:   [2.5731 µs 2.5785 µs 2.5850 µs]
parser/simple/solang    time:   [4.9611 µs 4.9650 µs 4.9693 µs]
parser/simple/slang     time:   [395.77 µs 398.76 µs 402.13 µs]

parser/verifier/sulk    time:   [134.47 µs 135.39 µs 136.32 µs]
parser/verifier/solang  time:   [547.88 µs 550.95 µs 555.08 µs]
parser/verifier/slang   time:   [37.924 ms 37.964 ms 38.006 ms]

parser/OptimizorClub/sulk
                        time:   [384.25 µs 385.17 µs 386.29 µs]
parser/OptimizorClub/solang
                        time:   [1.4672 ms 1.4794 ms 1.4930 ms]
parser/OptimizorClub/slang
                        time:   [110.48 ms 110.67 ms 110.83 ms]
```
