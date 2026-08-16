# Benchmark project corpus

This directory is the single source archive for the project benchmarks. Each archive is a
gzip-compressed Solidity Standard JSON input with inline source contents and the upstream build
settings needed by the benchmark that owns it.

The Criterion bench loads each archive directly, so CodSpeed and the runtime benchmark share the
same project files. The runtime harness selects an entrypoint's transitive import closure and
applies its runtime settings profile before compiling it. Large projects can therefore serve both
whole-project and focused runtime workloads without keeping a second extracted archive. Gungraun
keeps its existing benchmark set and is not expanded with these projects.

Several archives include upstream test sources. Criterion sees those files in its whole-project
parse, lowering, and codegen phases when the project enables that phase. The runtime benchmark
compiles only the selected test or production entrypoint and its imports.

The test-bearing archives are Forge Std, Morpho Blue, OpenZeppelin, PRBMath, Seaport, Solady,
Solarray, Solmate, and v4-core. The runtime suite currently exercises OpenZeppelin and Solady test
entrypoints; the other test trees remain whole-project Criterion inputs.

The runtime workload definitions and upstream revisions are documented in
[`../codegen-runtime/README.md`](../codegen-runtime/README.md). Runtime fixtures remain under
`../codegen-runtime/fixtures/`.
