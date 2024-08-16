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

Run with:
```sh
cargo criterion -p sulk-bench --bench bench -- --quiet --format terse --output-format bencher
# Copy paste the output into benches/tables.in and into the block below.
./benches/tables.py < benches/tables.in
```

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

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 11.815 ns | N/A     | N/A       |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 875.50 ns | N/A     | N/A       |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 30.396 µs | N/A     | N/A       |
| solang   | 93.020 ns | N/A     | N/A       |
| solc     | 1.0000 µs | N/A     | N/A       |
| sulk     | 1.1522 µs | N/A     | N/A       |

### Counter (14 LoC, 258 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 1.4874 µs | 9.41M   | 173.50M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 1.8424 µs | 7.60M   | 140.07M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 990.13 µs | 14.14K  | 260.57K   |
| solang   | 8.4306 µs | 1.66M   | 30.60M    |
| solc     | 22.400 µs | 625.00K | 11.52M    |
| sulk     | 3.7864 µs | 3.70M   | 68.15M    |

### verifier (208 LoC, 11040 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 62.957 µs | 3.30M   | 175.36M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 40.477 µs | 5.14M   | 272.75M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 63.894 ms | 3.26K   | 172.79K   |
| solang   | 436.96 µs | 476.02K | 25.27M    |
| solc     | 647.40 µs | 321.29K | 17.05M    |
| sulk     | 106.52 µs | 1.95M   | 103.64M   |

### OptimizorClub (782 LoC, 35905 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 195.80 µs | 3.99M   | 183.38M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 125.71 µs | 6.22M   | 285.62M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 192.11 ms | 4.07K   | 186.90K   |
| solang   | 1.3059 ms | 598.82K | 27.49M    |
| solc     | 1.5563 ms | 502.47K | 23.07M    |
| sulk     | 335.36 µs | 2.33M   | 107.06M   |

### UniswapV3 (3189 LoC, 146583 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 626.49 µs | 5.09M   | 233.98M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 373.85 µs | 8.53M   | 392.09M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 520.83 ms | 6.12K   | 281.44K   |
| solang   | 3.4100 ms | 935.19K | 42.99M    |
| solc     | 4.5205 ms | 705.45K | 32.43M    |
| sulk     | 824.85 µs | 3.87M   | 177.71M   |

### Solarray (1544 LoC, 35898 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 294.65 µs | 5.24M   | 121.83M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 223.38 µs | 6.91M   | 160.70M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 397.58 ms | 3.88K   | 90.29K    |
| solang   | 2.8652 ms | 538.88K | 12.53M    |
| solc     | 2.2148 ms | 697.13K | 16.21M    |
| sulk     | 660.48 µs | 2.34M   | 54.35M    |

### console (1552 LoC, 67315 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 440.98 µs | 3.52M   | 152.65M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 317.65 µs | 4.89M   | 211.92M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 451.94 ms | 3.43K   | 148.95K   |
| solang   | 3.6035 ms | 430.69K | 18.68M    |
| solc     | 3.3615 ms | 461.70K | 20.03M    |
| sulk     | 745.85 µs | 2.08M   | 90.25M    |

### Vm (1763 LoC, 91405 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 399.35 µs | 4.41M   | 228.88M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 212.46 µs | 8.30M   | 430.22M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 73.799 ms | 23.89K  | 1.24M     |
| solang   | 1.3489 ms | 1.31M   | 67.76M    |
| solc     | 2.4256 ms | 726.83K | 37.68M    |
| sulk     | 361.80 µs | 4.87M   | 252.64M   |

### safeconsole (13248 LoC, 397898 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 2.4099 ms | 5.50M   | 165.11M   |
| solc     | N/A       | N/A     | N/A       |
| sulk     | 1.7402 ms | 7.61M   | 228.65M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 3.7703 s  | 3.51K   | 105.53K   |
| solang   | 19.390 ms | 683.24K | 20.52M    |
| solc     | 19.213 ms | 689.50K | 20.71M    |
| sulk     | 5.8126 ms | 2.28M   | 68.45M    |
