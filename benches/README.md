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
parser/empty/solc/parse time:   [2.4235 ms 2.4321 ms 2.4406 ms]
parser/empty/sulk/lex   time:   [874.35 ns 875.50 ns 876.70 ns]
parser/empty/sulk/parse time:   [1.1508 µs 1.1522 µs 1.1534 µs]
parser/empty/solang/lex time:   [11.798 ns 11.815 ns 11.832 ns]
parser/empty/solang/parse
                        time:   [92.868 ns 93.020 ns 93.183 ns]
parser/empty/slang/parse
                        time:   [30.269 µs 30.396 µs 30.515 µs]

parser/Counter/solc/parse
                        time:   [2.4437 ms 2.4535 ms 2.4643 ms]
parser/Counter/sulk/lex time:   [1.8405 µs 1.8424 µs 1.8447 µs]
parser/Counter/sulk/parse
                        time:   [3.7812 µs 3.7864 µs 3.7919 µs]
parser/Counter/solang/lex
                        time:   [1.4832 µs 1.4874 µs 1.4915 µs]
parser/Counter/solang/parse
                        time:   [8.4250 µs 8.4306 µs 8.4359 µs]
parser/Counter/slang/parse
                        time:   [988.86 µs 990.13 µs 991.43 µs]

parser/verifier/solc/parse
                        time:   [3.0680 ms 3.0785 ms 3.0900 ms]
parser/verifier/sulk/lex
                        time:   [40.410 µs 40.477 µs 40.562 µs]
parser/verifier/sulk/parse
                        time:   [106.07 µs 106.52 µs 106.97 µs]
parser/verifier/solang/lex
                        time:   [62.735 µs 62.957 µs 63.121 µs]
parser/verifier/solang/parse
                        time:   [435.56 µs 436.96 µs 438.76 µs]
parser/verifier/slang/parse
                        time:   [63.825 ms 63.894 ms 63.968 ms]

parser/OptimizorClub/solc/parse
                        time:   [3.9752 ms 3.9874 ms 3.9988 ms]
parser/OptimizorClub/sulk/lex
                        time:   [124.55 µs 125.71 µs 126.86 µs]
parser/OptimizorClub/sulk/parse
                        time:   [334.97 µs 335.36 µs 335.82 µs]
parser/OptimizorClub/solang/lex
                        time:   [195.11 µs 195.80 µs 196.55 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.2984 ms 1.3059 ms 1.3135 ms]
parser/OptimizorClub/slang/parse
                        time:   [191.95 ms 192.11 ms 192.29 ms]

parser/UniswapV3/solc/parse
                        time:   [6.9351 ms 6.9516 ms 6.9671 ms]
parser/UniswapV3/sulk/lex
                        time:   [373.37 µs 373.85 µs 374.36 µs]
parser/UniswapV3/sulk/parse
                        time:   [821.62 µs 824.85 µs 829.47 µs]
parser/UniswapV3/solang/lex
                        time:   [623.89 µs 626.49 µs 628.89 µs]
parser/UniswapV3/solang/parse
                        time:   [3.4015 ms 3.4100 ms 3.4183 ms]
parser/UniswapV3/slang/parse
                        time:   [520.55 ms 520.83 ms 521.12 ms]

parser/Solarray/solc/parse
                        time:   [4.6263 ms 4.6459 ms 4.6646 ms]
parser/Solarray/sulk/lex
                        time:   [222.09 µs 223.38 µs 224.37 µs]
parser/Solarray/sulk/parse
                        time:   [659.86 µs 660.48 µs 661.26 µs]
parser/Solarray/solang/lex
                        time:   [293.12 µs 294.65 µs 296.55 µs]
parser/Solarray/solang/parse
                        time:   [2.8567 ms 2.8652 ms 2.8758 ms]
parser/Solarray/slang/parse
                        time:   [396.34 ms 397.58 ms 399.26 ms]

parser/console/solc/parse
                        time:   [5.7671 ms 5.7926 ms 5.8214 ms]
parser/console/sulk/lex time:   [315.71 µs 317.65 µs 319.56 µs]
parser/console/sulk/parse
                        time:   [742.73 µs 745.85 µs 749.71 µs]
parser/console/solang/lex
                        time:   [440.58 µs 440.98 µs 441.41 µs]
parser/console/solang/parse
                        time:   [3.6020 ms 3.6035 ms 3.6055 ms]
parser/console/slang/parse
                        time:   [450.77 ms 451.94 ms 452.95 ms]

parser/Vm/solc/parse    time:   [4.8402 ms 4.8567 ms 4.8749 ms]
parser/Vm/sulk/lex      time:   [212.08 µs 212.46 µs 212.80 µs]
parser/Vm/sulk/parse    time:   [361.31 µs 361.80 µs 362.34 µs]
parser/Vm/solang/lex    time:   [398.61 µs 399.35 µs 400.17 µs]
parser/Vm/solang/parse  time:   [1.3464 ms 1.3489 ms 1.3522 ms]
parser/Vm/slang/parse   time:   [73.688 ms 73.799 ms 73.919 ms]

parser/safeconsole/solc/parse
                        time:   [21.522 ms 21.645 ms 21.766 ms]
parser/safeconsole/sulk/lex
                        time:   [1.7355 ms 1.7402 ms 1.7443 ms]
parser/safeconsole/sulk/parse
                        time:   [5.7924 ms 5.8126 ms 5.8366 ms]
parser/safeconsole/solang/lex
                        time:   [2.4085 ms 2.4099 ms 2.4115 ms]
parser/safeconsole/solang/parse
                        time:   [19.179 ms 19.390 ms 19.600 ms]
parser/safeconsole/slang/parse
                        time:   [3.7491 s 3.7703 s 3.7915 s]
```

### empty (0 LoC, 0 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.0000008755    | 0.000001152      | N/A            | N/A              | 0               | 0                 |
| solang | 0.00000001182   | 0.00000009318    | N/A            | N/A              | 0               | 0                 |
| slang  | N/A             | 0.00003040       | N/A            | N/A              | 0               | 0                 |

### Counter (14 LoC, 258 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.000001843     | 0.000003786      | 7.598 M        | 140.0 M          | 3.698 M         | 68.15 M           |
| solang | 0.000001487     | 0.000008431      | 9.414 M        | 173.6 M          | 1.661 M         | 30.61 M           |
| slang  | N/A             | 0.0009901        | N/A            | N/A              | 141.4 K         | 2.608 M           |

### verifier (208 LoC, 11,040 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.00004047      | 0.0001065        | 5.139 M        | 272.3 M          | 1.958 M         | 103.6 M           |
| solang | 0.00006296      | 0.0004370        | 3.305 M        | 175.8 M          | 476.6 K         | 25.34 M           |
| slang  | N/A             | 0.06396          | N/A            | N/A              | 3.255 K         | 172.5 K           |

### OptimizorClub (782 LoC, 35,905 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.0001257       | 0.0003354        | 6.224 M        | 283.2 M          | 2.330 M         | 106.1 M           |
| solang | 0.0001958       | 0.001306         | 3.992 M        | 181.5 M          | 598.9 K         | 27.23 M           |
| slang  | N/A             | 0.1921           | N/A            | N/A              | 4.073 K         | 186.6 K           |

### UniswapV3 (3,189 LoC, 146,583 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.0003739       | 0.0008248        | 8.531 M        | 391.9 M          | 3.867 M         | 177.6 M           |
| solang | 0.0006265       | 0.003410         | 5.092 M        | 234.9 M          | 935.0 K         | 43.12 M           |
| slang  | N/A             | 0.5211           | N/A            | N/A              | 6.125 K         | 281.3 K           |

### Solarray (1,544 LoC, 35,898 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.0002234       | 0.0006605        | 6.911 M        | 160.7 M          | 2.338 M         | 54.35 M           |
| solang | 0.0002947       | 0.002865         | 5.241 M        | 121.9 M          | 538.7 K         | 12.52 M           |
| slang  | N/A             | 0.3993           | N/A            | N/A              | 3.887 K         | 90.14 K           |

### console (1,552 LoC, 67,315 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.0003177       | 0.0007459        | 4.887 M        | 211.3 M          | 2.082 M         | 89.64 M           |
| solang | 0.0004410       | 0.003604         | 3.520 M        | 152.7 M          | 430.6 K         | 18.70 M           |
| slang  | N/A             | 0.4520           | N/A            | N/A              | 3.433 K         | 148.9 K           |

### Vm (1,763 LoC, 91,405 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.0002125       | 0.0003618        | 8.297 M        | 429.9 M          | 4.872 M         | 252.3 M           |
| solang | 0.0003993       | 0.001349         | 4.414 M        | 229.3 M          | 1.306 M         | 62.56 M           |
| slang  | N/A             | 0.07380          | N/A            | N/A              | 23.91 K         | 1.151 M           |

### safeconsole (13,248 LoC, 397,898 bytes)

| Tool   | Lexing Time (s) | Parsing Time (s) | LoC/s (lexing) | Bytes/s (lexing) | LoC/s (parsing) | Bytes/s (parsing) |
|--------|-----------------|------------------|----------------|------------------|-----------------|-------------------|
| sulk   | 0.001742        | 0.005813         | 7.604 M        | 228.4 M          | 2.280 M         | 68.45 M           |
| solang | 0.002410        | 0.01939          | 5.495 M        | 165.0 M          | 682.9 K         | 20.49 M           |
| slang  | N/A             | 3.770            | N/A            | N/A              | 3.513 K         | 105.1 K           |
