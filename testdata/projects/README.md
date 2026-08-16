# Benchmark project corpus

This directory is the single source archive for the project benchmarks. Each archive is a
gzip-compressed Solidity Standard JSON input with inline source contents and the upstream build
settings needed by the benchmark that owns it.

The Criterion and Gungraun benches load the project sources directly. The codegen runtime harness
selects an entrypoint's transitive import closure and applies its runtime settings profile before
compiling it. Large projects can therefore serve both whole-project and focused runtime workloads
without keeping a second extracted archive.

The runtime workload definitions and upstream revisions are documented in
[`../codegen-runtime/README.md`](../codegen-runtime/README.md). Runtime fixtures remain under
`../codegen-runtime/fixtures/`.
