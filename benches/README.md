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
parser/empty/sulk/lex   time:   [417.58 ns 418.55 ns 419.57 ns]
parser/empty/sulk/parse time:   [700.78 ns 702.17 ns 704.00 ns]
parser/empty/solang/lex time:   [11.009 ns 11.074 ns 11.145 ns]
parser/empty/solang/parse
                        time:   [107.29 ns 107.57 ns 107.91 ns]
parser/empty/slang/parse
                        time:   [14.937 µs 14.959 µs 14.985 µs]

parser/simple/sulk/lex  time:   [965.39 ns 966.49 ns 967.76 ns]
parser/simple/sulk/parse
                        time:   [2.3744 µs 2.3788 µs 2.3832 µs]
parser/simple/solang/lex
                        time:   [1.0485 µs 1.0514 µs 1.0541 µs]
parser/simple/solang/parse
                        time:   [4.4007 µs 4.4056 µs 4.4106 µs]
parser/simple/slang/parse
                        time:   [365.15 µs 365.70 µs 366.28 µs]

parser/verifier/sulk/lex
                        time:   [35.739 µs 35.830 µs 35.918 µs]
parser/verifier/sulk/parse
                        time:   [127.55 µs 128.01 µs 128.59 µs]
parser/verifier/solang/lex
                        time:   [71.895 µs 72.229 µs 72.655 µs]
parser/verifier/solang/parse
                        time:   [448.01 µs 449.14 µs 450.38 µs]
parser/verifier/slang/parse
                        time:   [35.996 ms 36.076 ms 36.152 ms]

parser/OptimizorClub/sulk/lex
                        time:   [127.05 µs 127.30 µs 127.69 µs]
parser/OptimizorClub/sulk/parse
                        time:   [397.74 µs 398.73 µs 399.72 µs]
parser/OptimizorClub/solang/lex
                        time:   [211.62 µs 212.12 µs 212.54 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.2982 ms 1.3036 ms 1.3105 ms]
parser/OptimizorClub/slang/parse
                        time:   [110.09 ms 110.77 ms 111.44 ms]
```
