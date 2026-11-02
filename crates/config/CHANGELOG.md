# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Bug Fixes

- [config] Honor SOLAR_TEST_VERSION in any profile ([#2144](https://github.com/paradigmxyz/solar/issues/2144))
- [cli] Free the compiler in library calls ([#1884](https://github.com/paradigmxyz/solar/issues/1884))
- Address the review findings of the audit split ([#1383](https://github.com/paradigmxyz/solar/issues/1383))
- Compile external Foundry projects ([#1314](https://github.com/paradigmxyz/solar/issues/1314))
- [sema] Include referenced ABI items ([#1103](https://github.com/paradigmxyz/solar/issues/1103))
- [codegen] Match precompile targets ([#1073](https://github.com/paradigmxyz/solar/issues/1073))
- [config] Handle packaged version builds ([#922](https://github.com/paradigmxyz/solar/issues/922))
- [sema] Inherit public variable getter docs ([#825](https://github.com/paradigmxyz/solar/issues/825))

### Dependencies

- [deps] Replace vergen with smolgen ([#2033](https://github.com/paradigmxyz/solar/issues/2033))
- [deps] Bump vergen to 10 ([#461](https://github.com/paradigmxyz/solar/issues/461))

### Features

- Add the MIR interpreter and run-call check ([#1759](https://github.com/paradigmxyz/solar/issues/1759))
- Add the solar NatSpec tags ([#2101](https://github.com/paradigmxyz/solar/issues/2101))
- Add -Ztune=jit ([#2152](https://github.com/paradigmxyz/solar/issues/2152))
- [standard-json] Take codegen flags only from input ([#2143](https://github.com/paradigmxyz/solar/issues/2143))
- Name Solar builds in metadata and version output ([#2141](https://github.com/paradigmxyz/solar/issues/2141))
- Add solar-evmir and an EVM IR interpreter ([#1782](https://github.com/paradigmxyz/solar/issues/1782))
- [codegen] Add -Ztune for execution engines ([#2064](https://github.com/paradigmxyz/solar/issues/2064))
- [sema] Warn on unmatched version pragmas ([#1761](https://github.com/paradigmxyz/solar/issues/1761))
- [cli] Expose debug artifact outputs ([#1408](https://github.com/paradigmxyz/solar/issues/1408))
- Support Standard JSON `settings.debug` ([#1382](https://github.com/paradigmxyz/solar/issues/1382))
- [codegen] Implement Amsterdam EVM version ([#1288](https://github.com/paradigmxyz/solar/issues/1288))
- [test] Add external foundry testing infra ([#1213](https://github.com/paradigmxyz/solar/issues/1213))
- [cli] Warn instead of -Zcodegen ([#1169](https://github.com/paradigmxyz/solar/issues/1169))
- [cli] Expand standard JSON codegen support ([#1102](https://github.com/paradigmxyz/solar/issues/1102))
- [codegen] Add disassembly dumps ([#1077](https://github.com/paradigmxyz/solar/issues/1077))
- [cli] Merge IR optimization commands ([#1080](https://github.com/paradigmxyz/solar/issues/1080))
- [codegen] Lower clz and callcode ([#1046](https://github.com/paradigmxyz/solar/issues/1046))
- [codegen] Print pass diffs ([#1001](https://github.com/paradigmxyz/solar/issues/1001))
- [codegen] Add pass timing flag ([#962](https://github.com/paradigmxyz/solar/issues/962))
- [codegen] Emit final EVM IR ([#959](https://github.com/paradigmxyz/solar/issues/959))
- [parser] Recover incomplete input ([#942](https://github.com/paradigmxyz/solar/issues/942))
- [sema] Enable type checking by default ([#939](https://github.com/paradigmxyz/solar/issues/939))
- [codegen] Phased MIR lowering and a leaner backend ([#921](https://github.com/paradigmxyz/solar/issues/921))
- Add HIR stats flag ([#893](https://github.com/paradigmxyz/solar/issues/893))
- [lsp] Add initial language server support ([#870](https://github.com/paradigmxyz/solar/issues/870))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))
- Add solc-compatible wasm API ([#850](https://github.com/paradigmxyz/solar/issues/850))
- Allow warnings by code ([#837](https://github.com/paradigmxyz/solar/issues/837))
- [cli] Support standard json pipeline ([#829](https://github.com/paradigmxyz/solar/issues/829))
- [cli] Add tracing-samply ([#669](https://github.com/paradigmxyz/solar/issues/669))
- Switch default emitter kind to unicode ([#629](https://github.com/paradigmxyz/solar/issues/629))
- [sema] Experimental typeck ([#563](https://github.com/paradigmxyz/solar/issues/563))

### Miscellaneous Tasks

- Update Solidity to 0.8.38 ([#2021](https://github.com/paradigmxyz/solar/issues/2021))
- Update Solidity to 0.8.37 ([#1434](https://github.com/paradigmxyz/solar/issues/1434))
- Update solidity to 0.8.36 ([#931](https://github.com/paradigmxyz/solar/issues/931))
- Release 0.2.0

### Other

- [codegen] Use isle, egraph and decl MIR/EVMIR ([#1373](https://github.com/paradigmxyz/solar/issues/1373))

### Performance

- [codegen] Run two cheap cleanups at -O none ([#2136](https://github.com/paradigmxyz/solar/issues/2136))
- [codegen] Stop packing wide jump table entries ([#2066](https://github.com/paradigmxyz/solar/issues/2066))
- [config] Skip git HEAD tracking in debug builds ([#2055](https://github.com/paradigmxyz/solar/issues/2055))
- [cli] Skip dropping the compiler context ([#1613](https://github.com/paradigmxyz/solar/issues/1613))
- [codegen] Improve EVM stack scheduling ([#1305](https://github.com/paradigmxyz/solar/issues/1305))
- [codegen] Compact interprocedural codegen ([#1156](https://github.com/paradigmxyz/solar/issues/1156))
- [codegen] Improve switch lowering ([#1002](https://github.com/paradigmxyz/solar/issues/1002))
- [codegen] Improve pass ordering ([#1017](https://github.com/paradigmxyz/solar/issues/1017))

### Refactor

- [config] Remove unread -Zassert-planned-edge-spill-home ([#2188](https://github.com/paradigmxyz/solar/issues/2188))
- [codegen] Rewrite MIR codegen ([#1161](https://github.com/paradigmxyz/solar/issues/1161))
- [codegen] Share pass manager ([#999](https://github.com/paradigmxyz/solar/issues/999))
- [codegen] Remove pass options ([#1000](https://github.com/paradigmxyz/solar/issues/1000))
- [codegen] Streamline EVM IR pipeline ([#990](https://github.com/paradigmxyz/solar/issues/990))
- [codegen] Minimize assembler ([#982](https://github.com/paradigmxyz/solar/issues/982))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Features

- Checks on upper bounds of contract storage sizes ([#169](https://github.com/paradigmxyz/solar/issues/169))
- Add error format options for human-readable diagnostics ([#473](https://github.com/paradigmxyz/solar/issues/473))
- [interface] Impl Default for Session, create dcx from opts ([#471](https://github.com/paradigmxyz/solar/issues/471))
- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Release 0.1.8 ([#551](https://github.com/paradigmxyz/solar/issues/551))
- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))
- Release 0.1.7 ([#492](https://github.com/paradigmxyz/solar/issues/492))
- Move VERSION to config::version::SEMVER_VERSION and log it ([#454](https://github.com/paradigmxyz/solar/issues/454))
- Cap default threads at available_parallelism ([#445](https://github.com/paradigmxyz/solar/issues/445))
- Release 0.1.6 ([#443](https://github.com/paradigmxyz/solar/issues/443))

### Testing

- Add CLI test suite ([#453](https://github.com/paradigmxyz/solar/issues/453))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Features

- Add span visitor debug tool ([#355](https://github.com/paradigmxyz/solar/issues/355))
- Update solidity to 0.8.30 ([#307](https://github.com/paradigmxyz/solar/issues/307))
- `--no-warnings` ([#293](https://github.com/paradigmxyz/solar/issues/293))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))

### Miscellaneous Tasks

- Release 0.1.5 ([#382](https://github.com/paradigmxyz/solar/issues/382))
- Release 0.1.4 ([#341](https://github.com/paradigmxyz/solar/issues/341))
- Release 0.1.3 ([#310](https://github.com/paradigmxyz/solar/issues/310))
- Add some more non_exhaustive ([#261](https://github.com/paradigmxyz/solar/issues/261))
- Revert extra changelog

### Performance

- Add and default to using mimalloc ([#276](https://github.com/paradigmxyz/solar/issues/276))

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Dedup resolved files ([#246](https://github.com/paradigmxyz/solar/issues/246))
- Correct features in version string ([#242](https://github.com/paradigmxyz/solar/issues/242))
- Use custom build profile in --version ([#192](https://github.com/paradigmxyz/solar/issues/192))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))

### Miscellaneous Tasks

- Release 0.1.2
- Release 0.1.2 ([#251](https://github.com/paradigmxyz/solar/issues/251))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))

### Miscellaneous Tasks

- Release 0.1.1 ([#182](https://github.com/paradigmxyz/solar/issues/182))

## [0.2.0](https://github.com/paradigmxyz/solar/releases/tag/v0.2.0)

### Bug Fixes

- [config] Handle packaged version builds ([#922](https://github.com/paradigmxyz/solar/issues/922))
- [sema] Inherit public variable getter docs ([#825](https://github.com/paradigmxyz/solar/issues/825))

### Dependencies

- [deps] Bump vergen to 10 ([#461](https://github.com/paradigmxyz/solar/issues/461))

### Features

- Add HIR stats flag ([#893](https://github.com/paradigmxyz/solar/issues/893))
- [lsp] Add initial language server support ([#870](https://github.com/paradigmxyz/solar/issues/870))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))
- Add solc-compatible wasm API ([#850](https://github.com/paradigmxyz/solar/issues/850))
- Allow warnings by code ([#837](https://github.com/paradigmxyz/solar/issues/837))
- [cli] Support standard json pipeline ([#829](https://github.com/paradigmxyz/solar/issues/829))
- [cli] Add tracing-samply ([#669](https://github.com/paradigmxyz/solar/issues/669))
- Switch default emitter kind to unicode ([#629](https://github.com/paradigmxyz/solar/issues/629))
- [sema] Experimental typeck ([#563](https://github.com/paradigmxyz/solar/issues/563))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Features

- Checks on upper bounds of contract storage sizes ([#169](https://github.com/paradigmxyz/solar/issues/169))

### Miscellaneous Tasks

- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Features

- Add error format options for human-readable diagnostics ([#473](https://github.com/paradigmxyz/solar/issues/473))
- [interface] Impl Default for Session, create dcx from opts ([#471](https://github.com/paradigmxyz/solar/issues/471))

### Miscellaneous Tasks

- Move VERSION to config::version::SEMVER_VERSION and log it ([#454](https://github.com/paradigmxyz/solar/issues/454))
- Cap default threads at available_parallelism ([#445](https://github.com/paradigmxyz/solar/issues/445))

### Testing

- Add CLI test suite ([#453](https://github.com/paradigmxyz/solar/issues/453))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Features

- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Features

- Add span visitor debug tool ([#355](https://github.com/paradigmxyz/solar/issues/355))

## [Unreleased](https://github.com/paradigmxyz/solar/compare/v0.1.3...HEAD)

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Features

- Update solidity to 0.8.30 ([#307](https://github.com/paradigmxyz/solar/issues/307))
- `--no-warnings` ([#293](https://github.com/paradigmxyz/solar/issues/293))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))

### Miscellaneous Tasks

- Add some more non_exhaustive ([#261](https://github.com/paradigmxyz/solar/issues/261))
- Revert extra changelog

### Performance

- Add and default to using mimalloc ([#276](https://github.com/paradigmxyz/solar/issues/276))

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Dedup resolved files ([#246](https://github.com/paradigmxyz/solar/issues/246))
- Correct features in version string ([#242](https://github.com/paradigmxyz/solar/issues/242))
- Use custom build profile in --version ([#192](https://github.com/paradigmxyz/solar/issues/192))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
