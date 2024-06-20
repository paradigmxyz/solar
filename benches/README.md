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
`solc 0.8.25`, `sulk @ 6167e5e`, `solang-parser =0.3.3`, `slang =0.14.2`:

```
parser/empty/solc/parse time:   [2.4170 ms 2.4295 ms 2.4413 ms]
parser/empty/sulk/lex   time:   [493.65 ns 495.87 ns 498.45 ns]
parser/empty/sulk/parse time:   [697.62 ns 699.53 ns 701.55 ns]
parser/empty/solang/lex time:   [11.186 ns 11.247 ns 11.327 ns]
parser/empty/solang/parse
                        time:   [97.600 ns 98.221 ns 98.886 ns]
parser/empty/slang/parse
                        time:   [27.264 µs 27.297 µs 27.341 µs]

parser/simple/solc/parse
                        time:   [2.4513 ms 2.4581 ms 2.4647 ms]
parser/simple/sulk/lex  time:   [1.1135 µs 1.1150 µs 1.1167 µs]
parser/simple/sulk/parse
                        time:   [2.3592 µs 2.3659 µs 2.3750 µs]
parser/simple/solang/lex
                        time:   [1.0187 µs 1.0225 µs 1.0259 µs]
parser/simple/solang/parse
                        time:   [4.0313 µs 4.0446 µs 4.0595 µs]
parser/simple/slang/parse
                        time:   [591.38 µs 591.89 µs 592.38 µs]

parser/verifier/solc/parse
                        time:   [3.0417 ms 3.0533 ms 3.0647 ms]
parser/verifier/sulk/lex
                        time:   [37.896 µs 38.081 µs 38.356 µs]
parser/verifier/sulk/parse
                        time:   [118.60 µs 119.29 µs 119.81 µs]
parser/verifier/solang/lex
                        time:   [67.932 µs 68.332 µs 68.802 µs]
parser/verifier/solang/parse
                        time:   [425.96 µs 428.64 µs 430.76 µs]
parser/verifier/slang/parse
                        time:   [62.633 ms 62.723 ms 62.822 ms]

parser/OptimizorClub/solc/parse
                        time:   [3.9111 ms 3.9286 ms 3.9465 ms]
parser/OptimizorClub/sulk/lex
                        time:   [125.53 µs 126.36 µs 127.06 µs]
parser/OptimizorClub/sulk/parse
                        time:   [362.96 µs 365.13 µs 367.88 µs]
parser/OptimizorClub/solang/lex
                        time:   [208.67 µs 210.15 µs 212.34 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.2064 ms 1.2122 ms 1.2199 ms]
parser/OptimizorClub/slang/parse
                        time:   [189.28 ms 189.93 ms 190.59 ms]

parser/UniswapV3/solc/parse
                        time:   [6.9013 ms 6.9212 ms 6.9418 ms]
parser/UniswapV3/sulk/lex
                        time:   [361.79 µs 364.42 µs 367.35 µs]
parser/UniswapV3/sulk/parse
                        time:   [945.20 µs 945.89 µs 946.74 µs]
parser/UniswapV3/solang/lex
                        time:   [682.52 µs 685.63 µs 689.26 µs]
parser/UniswapV3/solang/parse
                        time:   [3.2683 ms 3.2786 ms 3.2896 ms]
parser/UniswapV3/slang/parse
                        time:   [517.57 ms 519.66 ms 521.78 ms]
```

### UniswapV3 - 3200 LoC

`solc 0.8.25`, `sulk @ 6167e5e`, `solang-parser =0.3.3`, `slang =0.14.2`:

|        | Lex (ms) | Lex (KLoC/s) | Parse (ms) | Parse (KLoC/s) |
| ------ | -------- | ------------ | ---------- | -------------- |
| solc   | N/A      | N/A          | 4.29       | 745.9          |
| sulk   | 0.36     | 8888.9       | 0.95       | 3368.4         |
| solang | 0.64     | 5000.0       | 3.29       | 972.6          |
| slang  | N/A      | N/A          | 518.79     | 6.2            |
