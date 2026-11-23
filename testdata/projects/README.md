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
Solarray, Solmate, Tempo Zones, and v4-core. The runtime suite currently exercises OpenZeppelin
and Solady test entrypoints; the other test trees remain whole-project inputs in the `projects` suite.

Runtime workload definitions and upstream revisions are documented in
[`../../benches/runtime/README.md`](../../benches/runtime/README.md).

## Tempo Zones

`tempo-zones.json.gz` captures `forge test --list`'s compiler input from
[`tempoxyz/zones`](https://github.com/tempoxyz/zones/tree/700069eb8a40fce7e1f7a4bf0fc5703bcc692c43)
under `crates/contracts/`: 67 sources, including 21 test files. It preserves the upstream
via-IR pipeline, 200 optimizer runs, Osaka EVM target, and Foundry output selection, including
storage layout. Unlike Forge's input, it sets `settings.metadata` to `appendCBOR: false` and
`bytecodeHash: none`, like the other archives, so unchanged codegen gives identical bytecode,
and requests each source's AST with `"": ["ast"]`, as the other archives do; set both again
after regenerating. Sources are inline, so the archive needs neither network access nor submodules.
No Solidity tests were executed to produce it.

The input was captured at the compiler handoff with Forge `1.6.0-v1.7.0`
(`f83bad912a9dba7bf0371def1e70bb1896048356`) and solc `0.8.35`.
`FOUNDRY_HARDFORK=cancun` lets that Forge parse the newer upstream `tempo:T8` setting;
the Standard JSON EVM target remains `osaka`. Initialize the pinned submodules before regenerating:
`tempo-std` at `96f882963ce3b0e0abc7ec2b41d03d42457877bb` and `forge-std` at
`bf647bd6046f2f7da30d0c2bf435e5c76a780c1b`. Every embedded source was checked against these
public repositories' pinned Git objects.

Run the compile-only comparison with:

```sh
uv run --project tools/compiler-diff compiler-diff runtime -- \
  --solar /absolute/path/to/solar --solc /absolute/path/to/solc \
  --mode compile-time --projects tempo-zones --compile-repeats 3
```

## Decompressors

`zstd-giant.json.gz` and `gzip-yul.json.gz` hold two MIT-licensed mainnet contracts, with sources
taken unchanged from their Sourcify chain 1 verifications:
[`ZstdGiant`](https://repo.sourcify.dev/1/0x2bC7404Ba5CAd16f7A01437A6e6a4325A72DCcFa) at
`0x2bC7404Ba5CAd16f7A01437A6e6a4325A72DCcFa` (`src/ZstdGiant.sol`) and
[`GzipYulWrapper`](https://repo.sourcify.dev/1/0x7B54d32da164dFDb06de3fDd4BC140662c6112C4) at
`0x7B54d32da164dFDb06de3fDd4BC140662c6112C4` (`src/GzipYulWrapper.sol` and `src/GzipYul.sol`).
Both were built with Solar 0.3.0 at 10,000 optimizer runs for Osaka, ZstdGiant through via-IR and
GzipYulWrapper through the legacy pipeline. The archives keep these settings and the
`forge-std/=lib/forge-std/src/` remapping, but set `settings.metadata` and the output selection
as the other archives do. To regenerate one, fetch
`https://sourcify.dev/server/v2/contract/1/<address>?fields=sources,compilation` and copy
`sources` and those compiler settings into a Standard JSON input. The runtime cases compile both
with the `runtime` profile instead.
