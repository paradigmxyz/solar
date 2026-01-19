# Compilation Time Benchmark: Solar vs solc

Average of 3 runs per contract.

| Contract | solc (s) | Solar (s) | Speedup |
|----------|----------|-----------|---------|
| arithmetic       |    0.379 |     0.299 |    1.3x |
| calls            |    0.511 |     0.345 |    1.5x |
| constructor-args |    0.529 |     0.357 |    1.5x |
| control-flow     |    0.494 |     0.424 |    1.2x |
| events           |    0.411 |     0.412 |    1.0x |
| multi-return     |    0.357 |     0.484 |    0.7x |
| stack-deep       |    0.298 |     0.380 |    0.8x |
| storage          |    0.288 |     0.497 |    0.6x |
|----------|----------|-----------|---------|
| **TOTAL**        |    3.267 |     3.198 |    1.0x |
