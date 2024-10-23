# solar-bench

Simple benchmarks across different Solidity parser implementations.

Run with:
```bash
# Criterion
cargo criterion -p solar-bench --bench bench

# iai - requires `valgrind` and `iai-callgrind-runner`
cargo bench -p solar-bench --bench iai
```

This crate is excluded from the main workspace to avoid compiling it (and its dependencies) when
invoking other commands such as `cargo test`.

## Results

For this compiler:
- ~3 µs to set and unset scoped-TLS; **not** included below.
- ~400 ns to setup and drop stderr emitter; included below in `lex`, `parse`.
- ~200 ns to setup and drop the parser; included below in `parse`.

In practice all of these are one-time costs.

Run with:
```sh
cargo criterion -p solar-bench --bench bench -- --quiet --format terse
# Copy paste the output into benches/tables.in and into the block below.
./benches/tables.py ./benches/README.md < benches/tables.in
```

Criterion results on `x86_64-unknown-linux-gnu` on AMD Ryzen 9 7950X;
`solc 0.8.28`, `solar @ 30b73e9`, `solang-parser =0.3.4`, `slang =0.18.2`:

```
parser/empty/solc/parse time:   [2.2922 ms 2.3108 ms 2.3282 ms]
parser/empty/solar/lex  time:   [541.36 ns 544.81 ns 549.85 ns]
parser/empty/solar/parse
                        time:   [845.16 ns 846.95 ns 848.78 ns]
parser/empty/solang/lex time:   [12.051 ns 12.088 ns 12.130 ns]
parser/empty/solang/parse
                        time:   [101.11 ns 101.31 ns 101.58 ns]
parser/empty/slang/parse
                        time:   [3.7528 µs 3.7603 µs 3.7663 µs]

parser/Counter/solc/parse
                        time:   [2.3552 ms 2.3659 ms 2.3759 ms]
parser/Counter/solar/lex
                        time:   [1.4206 µs 1.4252 µs 1.4314 µs]
parser/Counter/solar/parse
                        time:   [3.3760 µs 3.3853 µs 3.3934 µs]
parser/Counter/solang/lex
                        time:   [1.5712 µs 1.5804 µs 1.5870 µs]
parser/Counter/solang/parse
                        time:   [9.1701 µs 9.2166 µs 9.2867 µs]
parser/Counter/slang/parse
                        time:   [179.14 µs 181.72 µs 183.99 µs]

parser/verifier/solc/parse
                        time:   [2.9353 ms 2.9470 ms 2.9566 ms]
parser/verifier/solar/lex
                        time:   [37.789 µs 37.936 µs 38.087 µs]
parser/verifier/solar/parse
                        time:   [92.146 µs 92.462 µs 92.852 µs]
parser/verifier/solang/lex
                        time:   [70.038 µs 70.801 µs 71.580 µs]
parser/verifier/solang/parse
                        time:   [470.16 µs 472.12 µs 474.56 µs]
parser/verifier/slang/parse
                        time:   [9.9977 ms 10.026 ms 10.063 ms]

parser/OptimizorClub/solc/parse
                        time:   [3.8559 ms 3.8690 ms 3.8795 ms]
parser/OptimizorClub/solar/lex
                        time:   [124.86 µs 126.59 µs 127.85 µs]
parser/OptimizorClub/solar/parse
                        time:   [250.06 µs 256.01 µs 261.42 µs]
parser/OptimizorClub/solang/lex
                        time:   [193.60 µs 194.45 µs 195.80 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.3218 ms 1.3255 ms 1.3295 ms]
parser/OptimizorClub/slang/parse
                        time:   [27.246 ms 27.286 ms 27.327 ms]

parser/UniswapV3/solc/parse
                        time:   [6.8992 ms 6.9158 ms 6.9313 ms]
parser/UniswapV3/solar/lex
                        time:   [358.47 µs 361.62 µs 365.78 µs]
parser/UniswapV3/solar/parse
                        time:   [716.88 µs 720.27 µs 724.06 µs]
parser/UniswapV3/solang/lex
                        time:   [666.98 µs 670.52 µs 674.81 µs]
parser/UniswapV3/solang/parse
                        time:   [3.4199 ms 3.4409 ms 3.4574 ms]
parser/UniswapV3/slang/parse
                        time:   [73.188 ms 73.813 ms 74.258 ms]

parser/Solarray/solc/parse
                        time:   [4.4936 ms 4.5117 ms 4.5318 ms]
parser/Solarray/solar/lex
                        time:   [215.91 µs 216.19 µs 216.50 µs]
parser/Solarray/solar/parse
                        time:   [550.32 µs 551.84 µs 553.82 µs]
parser/Solarray/solang/lex
                        time:   [316.49 µs 317.62 µs 318.33 µs]
parser/Solarray/solang/parse
                        time:   [2.8547 ms 2.8561 ms 2.8573 ms]
parser/Solarray/slang/parse
                        time:   [67.971 ms 68.552 ms 69.321 ms]

parser/console/solc/parse
                        time:   [5.6462 ms 5.6751 ms 5.6990 ms]
parser/console/solar/lex
                        time:   [305.20 µs 305.93 µs 306.70 µs]
parser/console/solar/parse
                        time:   [640.79 µs 644.21 µs 648.02 µs]
parser/console/solang/lex
                        time:   [481.02 µs 481.54 µs 481.97 µs]
parser/console/solang/parse
                        time:   [3.6130 ms 3.6188 ms 3.6248 ms]
parser/console/slang/parse
                        time:   [74.032 ms 74.539 ms 75.131 ms]

parser/Vm/solc/parse    time:   [4.7578 ms 4.7860 ms 4.8090 ms]
parser/Vm/solar/lex     time:   [203.43 µs 203.56 µs 203.73 µs]
parser/Vm/solar/parse   time:   [336.94 µs 338.37 µs 340.39 µs]
parser/Vm/solang/lex    time:   [415.84 µs 431.17 µs 445.43 µs]
parser/Vm/solang/parse  time:   [1.4114 ms 1.4136 ms 1.4168 ms]
parser/Vm/slang/parse   time:   [19.430 ms 19.605 ms 19.742 ms]

parser/safeconsole/solc/parse
                        time:   [23.012 ms 23.179 ms 23.329 ms]
parser/safeconsole/solar/lex
                        time:   [1.6442 ms 1.6472 ms 1.6513 ms]
parser/safeconsole/solar/parse
                        time:   [4.6811 ms 4.7190 ms 4.7447 ms]
parser/safeconsole/solang/lex
                        time:   [2.5378 ms 2.5411 ms 2.5453 ms]
parser/safeconsole/solang/parse
                        time:   [19.103 ms 19.263 ms 19.464 ms]
parser/safeconsole/slang/parse
                        time:   [425.48 ms 428.78 ms 432.52 ms]
```

<!-- AUTOGENERATED MARKER -->

### empty (0 LoC, 0 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solang   | 1.00x      | 12.088 ns | N/A     | N/A       |
| solar    | 45.33x     | 544.81 ns | N/A     | N/A       |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solang   | 1.00x      | 101.31 ns | N/A     | N/A       |
| solar    | 8.38x      | 846.95 ns | N/A     | N/A       |
| solc     | 9.90x      | 1.0000 µs | N/A     | N/A       |
| slang    | 37.23x     | 3.7603 µs | N/A     | N/A       |

### Counter (14 LoC, 258 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 1.4252 µs | 9.82M   | 181.05M   |
| solang   | 1.11x      | 1.5804 µs | 8.86M   | 163.29M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 3.3853 µs | 4.14M   | 76.22M    |
| solang   | 2.72x      | 9.2166 µs | 1.52M   | 27.99M    |
| solc     | 16.57x     | 56.100 µs | 249.55K | 4.60M     |
| slang    | 53.68x     | 181.72 µs | 77.04K  | 1.42M     |

### verifier (208 LoC, 11040 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 37.936 µs | 5.48M   | 291.02M   |
| solang   | 1.87x      | 70.801 µs | 2.94M   | 155.93M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 92.462 µs | 2.25M   | 119.40M   |
| solang   | 5.11x      | 472.12 µs | 440.57K | 23.38M    |
| solc     | 6.89x      | 637.20 µs | 326.43K | 17.33M    |
| slang    | 108.43x    | 10.026 ms | 20.75K  | 1.10M     |

### OptimizorClub (782 LoC, 35905 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 126.59 µs | 6.18M   | 283.63M   |
| solang   | 1.54x      | 194.45 µs | 4.02M   | 184.65M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 256.01 µs | 3.05M   | 140.25M   |
| solang   | 5.18x      | 1.3255 ms | 589.97K | 27.09M    |
| solc     | 6.09x      | 1.5592 ms | 501.54K | 23.03M    |
| slang    | 106.58x    | 27.286 ms | 28.66K  | 1.32M     |

### UniswapV3 (3189 LoC, 146583 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 361.62 µs | 8.82M   | 405.35M   |
| solang   | 1.85x      | 670.52 µs | 4.76M   | 218.61M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 720.27 µs | 4.43M   | 203.51M   |
| solang   | 4.78x      | 3.4409 ms | 926.79K | 42.60M    |
| solc     | 6.39x      | 4.6060 ms | 692.36K | 31.82M    |
| slang    | 102.48x    | 73.813 ms | 43.20K  | 1.99M     |

### Solarray (1544 LoC, 35898 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 216.19 µs | 7.14M   | 166.05M   |
| solang   | 1.47x      | 317.62 µs | 4.86M   | 113.02M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 551.84 µs | 2.80M   | 65.05M    |
| solc     | 3.99x      | 2.2019 ms | 701.21K | 16.30M    |
| solang   | 5.18x      | 2.8561 ms | 540.60K | 12.57M    |
| slang    | 124.22x    | 68.552 ms | 22.52K  | 523.66K   |

### console (1552 LoC, 67315 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 305.93 µs | 5.07M   | 220.03M   |
| solang   | 1.57x      | 481.54 µs | 3.22M   | 139.79M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 644.21 µs | 2.41M   | 104.49M   |
| solc     | 5.22x      | 3.3653 ms | 461.18K | 20.00M    |
| solang   | 5.62x      | 3.6188 ms | 428.87K | 18.60M    |
| slang    | 115.71x    | 74.539 ms | 20.82K  | 903.08K   |

### Vm (1763 LoC, 91405 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 203.56 µs | 8.66M   | 449.03M   |
| solang   | 2.12x      | 431.17 µs | 4.09M   | 211.99M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 338.37 µs | 5.21M   | 270.13M   |
| solang   | 4.18x      | 1.4136 ms | 1.25M   | 64.66M    |
| solc     | 7.32x      | 2.4762 ms | 711.98K | 36.91M    |
| slang    | 57.94x     | 19.605 ms | 89.93K  | 4.66M     |

### safeconsole (13248 LoC, 397898 bytes)

#### Lex
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 1.6472 ms | 8.04M   | 241.56M   |
| solang   | 1.54x      | 2.5411 ms | 5.21M   | 156.58M   |

#### Parse
| Parser   | Relative   | Time      | LoC/s   | Bytes/s   |
|:---------|:-----------|:----------|:--------|:----------|
| solar    | 1.00x      | 4.7190 ms | 2.81M   | 84.32M    |
| solang   | 4.08x      | 19.263 ms | 687.74K | 20.66M    |
| solc     | 4.42x      | 20.869 ms | 634.81K | 19.07M    |
| slang    | 90.86x     | 428.78 ms | 30.90K  | 927.98K   |
