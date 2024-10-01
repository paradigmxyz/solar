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
./benches/tables.py < benches/tables.in
```

Criterion results on `x86_64-unknown-linux-gnu` on AMD Ryzen 7 7950X;
`solc 0.8.26`, `solar @ 8e3d642`, `solang-parser =0.3.4`, `slang =0.16.0`:

```
parser/empty/solc/parse time:   [1.3433 ms 1.3534 ms 1.3671 ms]
parser/empty/solar/lex   time:   [899.40 ns 900.60 ns 901.76 ns]
parser/empty/solar/parse time:   [1.2432 µs 1.2441 µs 1.2453 µs]
parser/empty/solang/lex time:   [12.175 ns 12.183 ns 12.196 ns]
parser/empty/solang/parse
                        time:   [99.264 ns 99.453 ns 99.668 ns]
parser/empty/slang/parse
                        time:   [30.699 µs 30.727 µs 30.751 µs]

parser/Counter/solc/parse
                        time:   [1.4660 ms 1.4716 ms 1.4775 ms]
parser/Counter/solar/lex time:   [1.9003 µs 1.9020 µs 1.9033 µs]
parser/Counter/solar/parse
                        time:   [3.6554 µs 3.6630 µs 3.6689 µs]
parser/Counter/solang/lex
                        time:   [1.5520 µs 1.5604 µs 1.5720 µs]
parser/Counter/solang/parse
                        time:   [8.4944 µs 8.5010 µs 8.5108 µs]
parser/Counter/slang/parse
                        time:   [967.39 µs 967.73 µs 967.97 µs]

parser/verifier/solc/parse
                        time:   [2.0360 ms 2.0447 ms 2.0518 ms]
parser/verifier/solar/lex
                        time:   [37.900 µs 37.958 µs 38.054 µs]
parser/verifier/solar/parse
                        time:   [90.800 µs 91.192 µs 91.608 µs]
parser/verifier/solang/lex
                        time:   [60.674 µs 60.760 µs 60.823 µs]
parser/verifier/solang/parse
                        time:   [456.22 µs 457.39 µs 458.84 µs]
parser/verifier/slang/parse
                        time:   [60.732 ms 60.795 ms 60.886 ms]

parser/OptimizorClub/solc/parse
                        time:   [2.9213 ms 2.9297 ms 2.9380 ms]
parser/OptimizorClub/solar/lex
                        time:   [119.12 µs 120.55 µs 122.04 µs]
parser/OptimizorClub/solar/parse
                        time:   [274.39 µs 279.00 µs 285.41 µs]
parser/OptimizorClub/solang/lex
                        time:   [191.73 µs 193.95 µs 196.28 µs]
parser/OptimizorClub/solang/parse
                        time:   [1.2964 ms 1.3101 ms 1.3258 ms]
parser/OptimizorClub/slang/parse
                        time:   [186.51 ms 186.62 ms 186.73 ms]

parser/UniswapV3/solc/parse
                        time:   [6.0144 ms 6.0342 ms 6.0483 ms]
parser/UniswapV3/solar/lex
                        time:   [348.32 µs 348.89 µs 349.74 µs]
parser/UniswapV3/solar/parse
                        time:   [733.32 µs 734.64 µs 736.06 µs]
parser/UniswapV3/solang/lex
                        time:   [668.94 µs 674.57 µs 679.37 µs]
parser/UniswapV3/solang/parse
                        time:   [3.4440 ms 3.4850 ms 3.5193 ms]
parser/UniswapV3/slang/parse
                        time:   [521.71 ms 523.17 ms 524.63 ms]

parser/Solarray/solc/parse
                        time:   [3.5923 ms 3.6048 ms 3.6189 ms]
parser/Solarray/solar/lex
                        time:   [211.54 µs 212.32 µs 213.56 µs]
parser/Solarray/solar/parse
                        time:   [559.89 µs 568.74 µs 575.52 µs]
parser/Solarray/solang/lex
                        time:   [294.11 µs 294.65 µs 295.15 µs]
parser/Solarray/solang/parse
                        time:   [2.7473 ms 2.7633 ms 2.7722 ms]
parser/Solarray/slang/parse
                        time:   [382.95 ms 384.38 ms 385.97 ms]

parser/console/solc/parse
                        time:   [4.6215 ms 4.6491 ms 4.6749 ms]
parser/console/solar/lex time:   [306.46 µs 308.51 µs 310.56 µs]
parser/console/solar/parse
                        time:   [653.96 µs 655.10 µs 656.24 µs]
parser/console/solang/lex
                        time:   [442.65 µs 445.00 µs 446.91 µs]
parser/console/solang/parse
                        time:   [3.5584 ms 3.5673 ms 3.5836 ms]
parser/console/slang/parse
                        time:   [422.64 ms 424.58 ms 426.62 ms]

parser/Vm/solc/parse    time:   [3.8192 ms 3.8359 ms 3.8494 ms]
parser/Vm/solar/lex      time:   [199.36 µs 199.64 µs 199.98 µs]
parser/Vm/solar/parse    time:   [356.30 µs 358.58 µs 360.07 µs]
parser/Vm/solang/lex    time:   [410.24 µs 411.61 µs 412.84 µs]
parser/Vm/solang/parse  time:   [1.3992 ms 1.4055 ms 1.4100 ms]
parser/Vm/slang/parse   time:   [73.307 ms 73.501 ms 73.675 ms]

parser/safeconsole/solc/parse
                        time:   [20.815 ms 21.013 ms 21.192 ms]
parser/safeconsole/solar/lex
                        time:   [1.6436 ms 1.6641 ms 1.6775 ms]
parser/safeconsole/solar/parse
                        time:   [5.1074 ms 5.1281 ms 5.1561 ms]
parser/safeconsole/solang/lex
                        time:   [2.4431 ms 2.4540 ms 2.4652 ms]
parser/safeconsole/solang/parse
                        time:   [21.561 ms 24.186 ms 28.122 ms]
parser/safeconsole/slang/parse
                        time:   [3.6266 s 3.6457 s 3.6646 s]
```

### empty (0 LoC, 0 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 12.183 ns | N/A     | N/A       |
| solc     | N/A       | N/A     | N/A       |
| solar     | 900.60 ns | N/A     | N/A       |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 30.727 µs | N/A     | N/A       |
| solang   | 99.453 ns | N/A     | N/A       |
| solc     | 1.0000 µs | N/A     | N/A       |
| solar     | 1.2441 µs | N/A     | N/A       |

### Counter (14 LoC, 258 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 1.5604 µs | 8.97M   | 165.38M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 1.9020 µs | 7.36M   | 135.65M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 967.73 µs | 14.47K  | 266.60K   |
| solang   | 8.5010 µs | 1.65M   | 30.35M    |
| solc     | 119.20 µs | 117.45K | 2.16M     |
| solar     | 3.6630 µs | 3.82M   | 70.43M    |

### verifier (208 LoC, 11040 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 60.760 µs | 3.42M   | 181.70M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 37.958 µs | 5.48M   | 290.85M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 60.795 ms | 3.42K   | 181.59K   |
| solang   | 457.39 µs | 454.75K | 24.14M    |
| solc     | 692.30 µs | 300.45K | 15.95M    |
| solar     | 91.192 µs | 2.28M   | 121.06M   |

### OptimizorClub (782 LoC, 35905 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 193.95 µs | 4.03M   | 185.13M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 120.55 µs | 6.49M   | 297.84M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 186.62 ms | 4.19K   | 192.40K   |
| solang   | 1.3101 ms | 596.90K | 27.41M    |
| solc     | 1.5773 ms | 495.78K | 22.76M    |
| solar     | 279.00 µs | 2.80M   | 128.69M   |

### UniswapV3 (3189 LoC, 146583 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 674.57 µs | 4.73M   | 217.30M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 348.89 µs | 9.14M   | 420.14M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 523.17 ms | 6.10K   | 280.18K   |
| solang   | 3.4850 ms | 915.06K | 42.06M    |
| solc     | 4.6818 ms | 681.15K | 31.31M    |
| solar     | 734.64 µs | 4.34M   | 199.53M   |

### Solarray (1544 LoC, 35898 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 294.65 µs | 5.24M   | 121.83M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 212.32 µs | 7.27M   | 169.07M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 384.38 ms | 4.02K   | 93.39K    |
| solang   | 2.7633 ms | 558.75K | 12.99M    |
| solc     | 2.2524 ms | 685.49K | 15.94M    |
| solar     | 568.74 µs | 2.71M   | 63.12M    |

### console (1552 LoC, 67315 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 445.00 µs | 3.49M   | 151.27M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 308.51 µs | 5.03M   | 218.19M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 424.58 ms | 3.66K   | 158.54K   |
| solang   | 3.5673 ms | 435.06K | 18.87M    |
| solc     | 3.2967 ms | 470.77K | 20.42M    |
| solar     | 655.10 µs | 2.37M   | 102.76M   |

### Vm (1763 LoC, 91405 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 411.61 µs | 4.28M   | 222.07M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 199.64 µs | 8.83M   | 457.85M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 73.501 ms | 23.99K  | 1.24M     |
| solang   | 1.4055 ms | 1.25M   | 65.03M    |
| solc     | 2.4835 ms | 709.89K | 36.80M    |
| solar     | 358.58 µs | 4.92M   | 254.91M   |

### safeconsole (13248 LoC, 397898 bytes)

#### Lex
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | N/A       | N/A     | N/A       |
| solang   | 2.4540 ms | 5.40M   | 162.14M   |
| solc     | N/A       | N/A     | N/A       |
| solar     | 1.6641 ms | 7.96M   | 239.11M   |

#### Parse
| Parser   | Time      | LoC/s   | Bytes/s   |
|:---------|:----------|:--------|:----------|
| slang    | 3.6457 s  | 3.63K   | 109.14K   |
| solang   | 24.186 ms | 547.75K | 16.45M    |
| solc     | 19.660 ms | 673.83K | 20.24M    |
| solar     | 5.1281 ms | 2.58M   | 77.59M    |
