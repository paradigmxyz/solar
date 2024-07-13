# sulk-bench

Simple benchmarks across different Solidity parser implementations.

Run with:
```bash
# Criterion
cargo criterion -p sulk-bench --bench bench

# iai - requires `valgrind` and `iai-callgrind-runner`
cargo bench -p sulk-bench --bench iai
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
`solc 0.8.26`, `sulk @ bb6bf9c`, `solang-parser =0.3.4`, `slang =0.15.1`:

```
parser/empty/solc/parse time:   [2.5280 ms 2.5402 ms 2.5517 ms]
parser/empty/sulk/lex   time:   [952.45 ns 953.81 ns 955.42 ns]
parser/empty/sulk/parse time:   [1.1322 µs 1.1340 µs 1.1359 µs]
parser/empty/solang/lex time:   [13.268 ns 13.295 ns 13.325 ns]
parser/empty/solang/parse
                        time:   [86.818 ns 87.008 ns 87.212 ns]
parser/empty/slang/parse
                        time:   [27.085 µs 27.237 µs 27.369 µs]

parser/simple/solc/parse
                        time:   [2.5730 ms 2.5866 ms 2.6003 ms]
parser/simple/sulk/lex  time:   [1.6517 µs 1.6567 µs 1.6624 µs]
parser/simple/sulk/parse
                        time:   [3.0076 µs 3.0128 µs 3.0195 µs]
parser/simple/solang/lex
                        time:   [995.35 ns 1.0016 µs 1.0097 µs]
parser/simple/solang/parse
                        time:   [4.7668 µs 4.7741 µs 4.7841 µs]
parser/simple/slang/parse
                        time:   [540.83 µs 542.02 µs 543.40 µs]

parser/verifier/solc/parse
                        time:   [3.1348 ms 3.1516 ms 3.1679 ms]
parser/verifier/sulk/lex
                        time:   [39.827 µs 40.277 µs 40.905 µs]
parser/verifier/sulk/parse
                        time:   [106.44 µs 106.67 µs 106.93 µs]
parser/verifier/solang/lex
                        time:   [66.020 µs 66.276 µs 66.534 µs]
parser/verifier/solang/parse
                        time:   [465.10 µs 467.44 µs 469.80 µs]
parser/verifier/slang/parse
                        time:   [59.519 ms 59.599 ms 59.685 ms]

parser/OptimizorClub/solc/parse
                        time:   [4.0854 ms 4.1130 ms 4.1425 ms]
parser/OptimizorClub/sulk/lex
                        time:   [125.88 µs 126.25 µs 126.55 µs]
parser/OptimizorClub/sulk/parse
                        time:   [302.20 µs 305.52 µs 309.28 µs]
parser/OptimizorClub/solang/lex
                        time:   [188.92 µs 190.76 µs 193.57 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.3228 ms 1.3291 ms 1.3348 ms]
parser/OptimizorClub/slang/parse
                        time:   [175.26 ms 175.52 ms 175.80 ms]

parser/UniswapV3/solc/parse
                        time:   [7.0512 ms 7.0864 ms 7.1185 ms]
parser/UniswapV3/sulk/lex
                        time:   [353.43 µs 354.17 µs 355.02 µs]
parser/UniswapV3/sulk/parse
                        time:   [843.90 µs 846.21 µs 848.73 µs]
parser/UniswapV3/solang/lex
                        time:   [652.40 µs 655.82 µs 660.62 µs]
parser/UniswapV3/solang/parse
                        time:   [3.4569 ms 3.4636 ms 3.4710 ms]
parser/UniswapV3/slang/parse
                        time:   [502.72 ms 503.38 ms 504.10 ms]
```

### UniswapV3 - 3200 LoC

`solc 0.8.25`, `sulk @ 6167e5e`, `solang-parser =0.3.3`, `slang =0.14.2`:

|        | Lex (ms) | Lex (KLoC/s) | Parse (ms) | Parse (KLoC/s) |
| ------ | -------- | ------------ | ---------- | -------------- |
| solc   | N/A      | N/A          | 4.29       | 745.9          |
| sulk   | 0.36     | 8888.9       | 0.95       | 3368.4         |
| solang | 0.64     | 5000.0       | 3.29       | 972.6          |
| slang  | N/A      | N/A          | 518.79     | 6.2            |
