# sulk-bench

Simple benchmarks across different Solidity parser implementations.

Run with:
```bash
# Criterion
cargo r -r -p sulk-bench -- --bench

# Valgrind
# See `--valgrind --help`
cargo b -r -p sulk-bench
valgrind --tool=callgrind target/release/sulk-bench --valgrind
```

This crate is excluded from the main workspace to avoid compiling it (and its dependencies) when
invoking other commands such as `cargo test`.

## Results

For this compiler:
- ~3 µs to set and unset scoped-TLS; **not** included below.
- ~400 ns to setup and drop stderr emitter; included below in `lex`, `parse`.
- ~200 ns to setup and drop the parser; included below in `parse`.

In practice all of these are one-time costs.

Criterion results on `x86_64-unknown-linux-gnu` on AMD Ryzen 7 7950X:

```
parser/empty/sulk/lex   time:   [407.36 ns 408.68 ns 410.18 ns]
parser/empty/sulk/parse time:   [685.35 ns 690.52 ns 695.61 ns]
parser/empty/solang/lex time:   [10.986 ns 11.014 ns 11.046 ns]
parser/empty/solang/parse
                        time:   [110.21 ns 110.67 ns 111.09 ns]
parser/empty/slang/parse
                        time:   [14.577 µs 14.602 µs 14.632 µs]

parser/simple/sulk/lex  time:   [1.2385 µs 1.2437 µs 1.2507 µs]
parser/simple/sulk/parse
                        time:   [2.4507 µs 2.4683 µs 2.4871 µs]
parser/simple/solang/lex
                        time:   [1.1127 µs 1.1168 µs 1.1207 µs]
parser/simple/solang/parse
                        time:   [4.2663 µs 4.2714 µs 4.2773 µs]
parser/simple/slang/parse
                        time:   [356.34 µs 356.69 µs 357.14 µs]

parser/verifier/sulk/lex
                        time:   [48.672 µs 48.803 µs 48.933 µs]
parser/verifier/sulk/parse
                        time:   [125.80 µs 126.37 µs 127.03 µs]
parser/verifier/solang/lex
                        time:   [72.662 µs 72.828 µs 72.997 µs]
parser/verifier/solang/parse
                        time:   [449.12 µs 449.84 µs 450.52 µs]
parser/verifier/slang/parse
                        time:   [37.542 ms 37.598 ms 37.660 ms]

parser/OptimizorClub/sulk/lex
                        time:   [164.54 µs 164.88 µs 165.24 µs]
parser/OptimizorClub/sulk/parse
                        time:   [398.59 µs 400.89 µs 402.81 µs]
parser/OptimizorClub/solang/lex
                        time:   [219.47 µs 220.07 µs 220.85 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.2791 ms 1.2815 ms 1.2844 ms]
parser/OptimizorClub/slang/parse
                        time:   [113.90 ms 114.07 ms 114.25 ms]
```
