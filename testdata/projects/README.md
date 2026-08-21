# Benchmark project archives

This directory stores the project benchmark inputs. Each archive is a gzip-compressed Solidity
Standard JSON input with inline source contents and the upstream build settings for its benchmark.

Criterion and CodSpeed load these archives directly for their existing Solar-only workloads. The
runtime benchmark's `heavy` suite passes each archive
unchanged to both solc and Solar for a whole-project compile-time comparison. Its other suites
select an entrypoint's transitive import closure and apply a runtime settings profile before
compiling it. Large projects can therefore serve both whole-project and focused runtime workloads
without keeping a second extracted archive.
Gungraun keeps its existing benchmark set and is not expanded with these projects.

Several archives include upstream test sources. Criterion sees those files in its whole-project
parse, lowering, and codegen phases when the project enables that phase. These are the files in
each pinned project's benchmark profile, not a promise to include every file in the repository.
For example, Solady's default profile excludes its EIP-7702, transient-storage, Ithaca, and
ZKsync paths. The focused runtime suites compile only the selected test or production entrypoint
and its imports.

The test-bearing archives are Forge Std, Morpho Blue, OpenZeppelin, PRBMath, Seaport, Solady,
Solarray, Solmate, and v4-core. The runtime suite currently exercises OpenZeppelin and Solady test
entrypoints; the other test trees remain whole-project inputs in the `projects` suite.

Runtime workload definitions and upstream revisions are documented in
[`../../benches/runtime/README.md`](../../benches/runtime/README.md).
