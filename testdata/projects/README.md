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
[`tempoxyz/tempo`](https://github.com/tempoxyz/tempo/tree/a86fa1ef623b756e7194638579470771f21850ee/crates/zones/contracts)
under `crates/zones/contracts/`: 69 sources, including 23 test files. It preserves the upstream
via-IR pipeline, 200 optimizer runs, Osaka EVM target, and Foundry output selection, including
storage layout. Unlike Forge's input, it sets `settings.metadata` to `appendCBOR: false` and
`bytecodeHash: none`, like the other archives, so unchanged codegen gives identical bytecode,
and requests each source's AST with `"": ["ast"]`, as the other archives do; set both again
after regenerating. Sources are inline, with paths relative to `crates/zones/contracts`, so the
archive needs neither network access nor submodules. No Solidity tests were executed to produce it.

The input was captured at the compiler handoff with Forge `1.9.0`
(`6056c5845d34205a83cf7108042adb137325c9df`) and solc `0.8.35`: `FOUNDRY_SOLC` pointed at a wrapper
that copied standard input to a file and ran solc. `forge test --list` requests only `abi`, so the
archive gives every source Forge's full output selection, which `forge test` sends for the sources
it compiles. Initialize the pinned submodules before regenerating:
`tempo-std` at `96f882963ce3b0e0abc7ec2b41d03d42457877bb` and `forge-std` at
`bf647bd6046f2f7da30d0c2bf435e5c76a780c1b`. Every embedded source was checked against the pinned
Git objects of `tempo` and these public repositories. The archive holds no local paths and only
the `language`, `settings`, and `sources` root keys.

Run the compile-only comparison with:

```sh
uv run --project tools/compiler-diff compiler-diff runtime -- \
  --solar /absolute/path/to/solar --solc /absolute/path/to/solc \
  --mode compile-time --projects tempo-zones --compile-repeats 3
```

## Tempo TIPs verify

`tempo-tips-verify.json.gz` captures the compiler input of `forge build` for the `src/` tree of
[`tips/verify`](https://github.com/tempoxyz/tempo/tree/0c061d9178d982863fe5e57c5863374c4215eecc/tips/verify)
in `tempoxyz/tempo`: 4 sources of its own, including the Solidity reference of the TIP-1034 channel
reserve and the abstract `ZoneFactory.sol`, and the 17 `tempo-std` files they import. It keeps the
upstream via-IR pipeline, optimizer, and remappings, with the EVM target Forge 1.9.0 picked (Osaka),
and the same `settings.metadata` and `"": ["ast"]` changes as the Tempo Zones archive; set both again
after regenerating. The tests and scripts are not in it. Sources are inline, and each matches its
pinned Git object: `tempo` at `0c061d9178d982863fe5e57c5863374c4215eecc`, and `tempo-std` at
`c6c89f5aaf17ff9fabc3e76d05a8e5ebc1e05318`, the `tips/verify/lib/tempo-std` submodule.

Capture it with `forge build --build-info` in a copy of `tips/verify` without its `test/` directory,
with `tempo-std` initialized and `FOUNDRY_SOLC` set to solc 0.8.39; the archive is the build info's
`input`.

## Tempo Earn

`tempo-earn.json.gz` holds the sources that [`tempoxyz/earn`](https://github.com/tempoxyz/earn/tree/d7d85f560d07d9827858ae4dd528c76debe713a5)'s
`EarnVault`, `HoldEngine`, `EarnVaultMocks.sol`, and `EarnVaultTestDeployer.sol` import: 22 Earn
sources, including the two test helpers, and 24 OpenZeppelin 5.3.0 files
(`e4f70216d759d8e6a64144a9e1f7bbeed78e7079`), 46 in all. Settings follow Earn's `foundry.toml`: via-IR,
200 optimizer runs, and its remappings, with Cancun as the EVM target, and the same
`settings.metadata` and `"": ["ast"]` changes as the other archives. It is not Earn's whole Forge
input: that has 337 sources, takes solc 0.8.39 over three minutes, and Forge rewrites `new EarnVault()`
in test sources to `deployCode` cheatcode calls. The archive keeps the files as they are in Git, each
checked against the pinned `earn` and `openzeppelin-contracts` objects. The runtime case
[`tempo-earn-vault`](../../benches/runtime/README.md#tempo-earn-gas) layers
[`TempoEarn.sol`](../runtime/TempoEarn.sol) over it.
