# sulk-bench

Simple benchmarks across different Solidity parser implementations.

Run with:
```bash
# Criterion
cargo criterion -p sulk-benches --bench bench

# Valgrind
# See `--valgrind --help`
cargo b --profile bench -p sulk-bench
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

Criterion results on `x86_64-unknown-linux-gnu` on AMD Ryzen 7 7950X;
`sulk-parse @ `, `solang-parser =0.3.3`, `slang =0.14.0`:

```
parser/empty/sulk/lex   time:   [442.21 ns 442.77 ns 443.41 ns]
parser/empty/sulk/parse time:   [617.24 ns 618.31 ns 619.57 ns]
parser/empty/solang/lex time:   [11.338 ns 11.436 ns 11.549 ns]
parser/empty/solang/parse
                        time:   [90.352 ns 90.608 ns 90.980 ns]
parser/empty/slang/parse
                        time:   [25.663 µs 25.680 µs 25.700 µs]

parser/simple/sulk/lex  time:   [1.0291 µs 1.0324 µs 1.0357 µs]
parser/simple/sulk/parse
                        time:   [2.4432 µs 2.4450 µs 2.4472 µs]
parser/simple/solang/lex
                        time:   [1.0999 µs 1.1064 µs 1.1133 µs]
parser/simple/solang/parse
                        time:   [4.1265 µs 4.1353 µs 4.1445 µs]
parser/simple/slang/parse
                        time:   [584.02 µs 584.37 µs 584.81 µs]

parser/verifier/sulk/lex
                        time:   [36.543 µs 36.624 µs 36.713 µs]
parser/verifier/sulk/parse
                        time:   [124.73 µs 125.47 µs 126.75 µs]
parser/verifier/solang/lex
                        time:   [70.687 µs 70.930 µs 71.236 µs]
parser/verifier/solang/parse
                        time:   [420.98 µs 422.68 µs 424.56 µs]
parser/verifier/slang/parse
                        time:   [63.713 ms 63.754 ms 63.800 ms]

parser/OptimizorClub/sulk/lex
                        time:   [122.92 µs 123.09 µs 123.30 µs]
parser/OptimizorClub/sulk/parse
                        time:   [367.79 µs 369.48 µs 370.97 µs]
parser/OptimizorClub/solang/lex
                        time:   [212.57 µs 212.99 µs 213.37 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.2452 ms 1.2513 ms 1.2576 ms]
parser/OptimizorClub/slang/parse
                        time:   [185.74 ms 186.10 ms 186.51 ms]
```
