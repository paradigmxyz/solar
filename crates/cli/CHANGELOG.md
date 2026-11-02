# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Bug Fixes

- [cli] Free the compiler in library calls ([#1884](https://github.com/paradigmxyz/solar/issues/1884))
- [interface] Resolve imports like solc ([#1740](https://github.com/paradigmxyz/solar/issues/1740))
- [parser] Raise the recursion limit to 256 ([#1704](https://github.com/paradigmxyz/solar/issues/1704))
- [interface] Use `/` in resolved names ([#1736](https://github.com/paradigmxyz/solar/issues/1736))
- [cli] Print source names verbatim ([#1702](https://github.com/paradigmxyz/solar/issues/1702))
- [interface] Name callback imports by source unit ([#1731](https://github.com/paradigmxyz/solar/issues/1731))
- [cli] Keep backslashes in standard JSON names ([#1728](https://github.com/paradigmxyz/solar/issues/1728))
- Preserve empty source output on errors ([#1591](https://github.com/paradigmxyz/solar/issues/1591))
- [cli] Emit requested empty method identifiers ([#1589](https://github.com/paradigmxyz/solar/issues/1589))
- [cli] Ignore unknown standard JSON fields ([#1574](https://github.com/paradigmxyz/solar/issues/1574))
- Preserve library address relocations ([#1427](https://github.com/paradigmxyz/solar/issues/1427))
- Exclude runtime IR from creation dumps ([#1448](https://github.com/paradigmxyz/solar/issues/1448))
- [codegen] Legalize legacy EVM shifts ([#1267](https://github.com/paradigmxyz/solar/issues/1267))
- [cli] Correct std json optimizer mapping
- [codegen] Harden optimizer semantics ([#1021](https://github.com/paradigmxyz/solar/issues/1021))

### Dependencies

- [deps] Drop duplicate and unused dependencies ([#1622](https://github.com/paradigmxyz/solar/issues/1622))
- Replace cfg-if with cfg_select ([#1612](https://github.com/paradigmxyz/solar/issues/1612))
- [codegen] Centralize contract codegen ([#1038](https://github.com/paradigmxyz/solar/issues/1038))

### Features

- Add the MIR interpreter and run-call check ([#1759](https://github.com/paradigmxyz/solar/issues/1759))
- Add the solar NatSpec tags ([#2101](https://github.com/paradigmxyz/solar/issues/2101))
- [standard-json] Key cborAuxdata by contract ([#2149](https://github.com/paradigmxyz/solar/issues/2149))
- [standard-json] Take codegen flags only from input ([#2143](https://github.com/paradigmxyz/solar/issues/2143))
- [standard-json] Report CBOR auxdata positions ([#2142](https://github.com/paradigmxyz/solar/issues/2142))
- Name Solar builds in metadata and version output ([#2141](https://github.com/paradigmxyz/solar/issues/2141))
- Add solar-evmir and an EVM IR interpreter ([#1782](https://github.com/paradigmxyz/solar/issues/1782))
- [mir] Enforce semantic lowering boundaries ([#1392](https://github.com/paradigmxyz/solar/issues/1392))
- [cli] Expose debug artifact outputs ([#1408](https://github.com/paradigmxyz/solar/issues/1408))
- [codegen] Guard non-view library functions ([#1385](https://github.com/paradigmxyz/solar/issues/1385))
- Support Standard JSON `settings.debug` ([#1382](https://github.com/paradigmxyz/solar/issues/1382))
- Audit and emit optimized debug info ([#1325](https://github.com/paradigmxyz/solar/issues/1325))
- [cli] Add bytecode metadata ([#1344](https://github.com/paradigmxyz/solar/issues/1344))
- [cli] Highlight IR console output ([#1317](https://github.com/paradigmxyz/solar/issues/1317))
- [codegen] Implement Amsterdam EVM version ([#1288](https://github.com/paradigmxyz/solar/issues/1288))
- [lsp] Add embeddable launch API ([#1209](https://github.com/paradigmxyz/solar/issues/1209))
- [cli] Warn instead of -Zcodegen ([#1169](https://github.com/paradigmxyz/solar/issues/1169))
- [cli] Expand standard JSON codegen support ([#1102](https://github.com/paradigmxyz/solar/issues/1102))
- [codegen] Add disassembly dumps ([#1077](https://github.com/paradigmxyz/solar/issues/1077))
- [cli] Merge IR optimization commands ([#1080](https://github.com/paradigmxyz/solar/issues/1080))
- Introduce new memory model ([#1004](https://github.com/paradigmxyz/solar/issues/1004))
- [codegen] Print pass diffs ([#1001](https://github.com/paradigmxyz/solar/issues/1001))
- [codegen] Add pass timing flag ([#962](https://github.com/paradigmxyz/solar/issues/962))
- [codegen] Emit final EVM IR ([#959](https://github.com/paradigmxyz/solar/issues/959))
- [cli] Color standard JSON diagnostics ([#936](https://github.com/paradigmxyz/solar/issues/936))
- [codegen] Phased MIR lowering and a leaner backend ([#921](https://github.com/paradigmxyz/solar/issues/921))
- [cli] Emit docs and storage layout ([#933](https://github.com/paradigmxyz/solar/issues/933))
- Introduce EVM IR ([#868](https://github.com/paradigmxyz/solar/issues/868))
- [lsp] Add initial language server support ([#870](https://github.com/paradigmxyz/solar/issues/870))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))
- Add solc-compatible wasm API ([#850](https://github.com/paradigmxyz/solar/issues/850))
- Allow warnings by code ([#837](https://github.com/paradigmxyz/solar/issues/837))
- [cli] Support standard json pipeline ([#829](https://github.com/paradigmxyz/solar/issues/829))
- [cli] Add tracing-samply ([#669](https://github.com/paradigmxyz/solar/issues/669))

### Miscellaneous Tasks

- Update Solidity to 0.8.38 ([#2021](https://github.com/paradigmxyz/solar/issues/2021))
- Release 0.2.0
- Dep upgrades ([#852](https://github.com/paradigmxyz/solar/issues/852))
- Warn instead err on invalid natspec tag ([#600](https://github.com/paradigmxyz/solar/issues/600))
- [lowering] Allow empty sources ([#572](https://github.com/paradigmxyz/solar/issues/572))

### Performance

- [cli] Use mimalloc v2 ([#2129](https://github.com/paradigmxyz/solar/issues/2129))
- [cli] Serialize standard JSON contracts in parallel ([#2023](https://github.com/paradigmxyz/solar/issues/2023))
- [codegen] Record debug info as location runs ([#1632](https://github.com/paradigmxyz/solar/issues/1632))
- [codegen] Defer embedded contract bytecode ([#1615](https://github.com/paradigmxyz/solar/issues/1615))
- [cli] Skip dropping the compiler context ([#1613](https://github.com/paradigmxyz/solar/issues/1613))
- [cli] Compute contract metadata during codegen ([#1605](https://github.com/paradigmxyz/solar/issues/1605))
- [cli] Parallelize standard JSON outputs ([#1602](https://github.com/paradigmxyz/solar/issues/1602))
- [codegen] Improve EVM stack scheduling ([#1305](https://github.com/paradigmxyz/solar/issues/1305))
- [codegen] Compact interprocedural codegen ([#1156](https://github.com/paradigmxyz/solar/issues/1156))
- [codegen] Improve pass ordering ([#1017](https://github.com/paradigmxyz/solar/issues/1017))
- [codegen] Use dense sets for IDs ([#958](https://github.com/paradigmxyz/solar/issues/958))

### Refactor

- [mir] Share the SCC search and drop dead plumbing ([#2192](https://github.com/paradigmxyz/solar/issues/2192))
- Restructure tracing spans ([#1633](https://github.com/paradigmxyz/solar/issues/1633))
- [cli] Reuse source map read guard ([#1614](https://github.com/paradigmxyz/solar/issues/1614))
- [cli] Scope metadata precomputation ([#1608](https://github.com/paradigmxyz/solar/issues/1608))
- Streamline solc source map encoding ([#1558](https://github.com/paradigmxyz/solar/issues/1558))
- [codegen] Organize MIR and split EVM codegen ([#1401](https://github.com/paradigmxyz/solar/issues/1401))
- [codegen] Rewrite MIR codegen ([#1161](https://github.com/paradigmxyz/solar/issues/1161))
- [codegen] Centralize MIR dumps ([#1051](https://github.com/paradigmxyz/solar/issues/1051))
- [codegen] Share pass manager ([#999](https://github.com/paradigmxyz/solar/issues/999))
- [codegen] Remove pass options ([#1000](https://github.com/paradigmxyz/solar/issues/1000))
- [codegen] Streamline EVM IR pipeline ([#990](https://github.com/paradigmxyz/solar/issues/990))
- [codegen] Minimize assembler ([#982](https://github.com/paradigmxyz/solar/issues/982))
- [codegen] Reuse parser infra for IR ([#971](https://github.com/paradigmxyz/solar/issues/971))
- [codegen] Require EVM IR lowering ([#967](https://github.com/paradigmxyz/solar/issues/967))
- [codegen] Normalize EVM IR ([#965](https://github.com/paradigmxyz/solar/issues/965))
- [codegen] Emit verifier diagnostics ([#961](https://github.com/paradigmxyz/solar/issues/961))

### Testing

- Integrate local debug-info differentials ([#1390](https://github.com/paradigmxyz/solar/issues/1390))
- Use standard codegen matrix ([#1342](https://github.com/paradigmxyz/solar/issues/1342))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Features

- Configurable logging destination ([#498](https://github.com/paradigmxyz/solar/issues/498))
- [interface] Impl Default for Session, create dcx from opts ([#471](https://github.com/paradigmxyz/solar/issues/471))
- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Release 0.1.8 ([#551](https://github.com/paradigmxyz/solar/issues/551))
- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))
- Release 0.1.7 ([#492](https://github.com/paradigmxyz/solar/issues/492))
- Update signal handler ([#463](https://github.com/paradigmxyz/solar/issues/463))
- Release 0.1.6 ([#443](https://github.com/paradigmxyz/solar/issues/443))
- Rename enter to enter_sequential ([#392](https://github.com/paradigmxyz/solar/issues/392))

### Performance

- Load input source files in parallel ([#429](https://github.com/paradigmxyz/solar/issues/429))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Features

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

- Correct features in version string ([#242](https://github.com/paradigmxyz/solar/issues/242))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Allow compiling out tracing-subscriber ([#213](https://github.com/paradigmxyz/solar/issues/213))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))

### Miscellaneous Tasks

- Release 0.1.2
- Release 0.1.2 ([#251](https://github.com/paradigmxyz/solar/issues/251))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Add Session::enter_parallel ([#183](https://github.com/paradigmxyz/solar/issues/183))
- Install rayon pool in Session::enter ([#123](https://github.com/paradigmxyz/solar/issues/123))

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))
- Add more methods to index types ([#156](https://github.com/paradigmxyz/solar/issues/156))
- Print AST statistics with -Zast-stats flag ([#125](https://github.com/paradigmxyz/solar/issues/125))

### Miscellaneous Tasks

- Release 0.1.1 ([#182](https://github.com/paradigmxyz/solar/issues/182))

## [0.2.0](https://github.com/paradigmxyz/solar/releases/tag/v0.2.0)

### Features

- Introduce EVM IR ([#868](https://github.com/paradigmxyz/solar/issues/868))
- [lsp] Add initial language server support ([#870](https://github.com/paradigmxyz/solar/issues/870))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))
- Add solc-compatible wasm API ([#850](https://github.com/paradigmxyz/solar/issues/850))
- Allow warnings by code ([#837](https://github.com/paradigmxyz/solar/issues/837))
- [cli] Support standard json pipeline ([#829](https://github.com/paradigmxyz/solar/issues/829))
- [cli] Add tracing-samply ([#669](https://github.com/paradigmxyz/solar/issues/669))

### Miscellaneous Tasks

- Dep upgrades ([#852](https://github.com/paradigmxyz/solar/issues/852))
- Warn instead err on invalid natspec tag ([#600](https://github.com/paradigmxyz/solar/issues/600))
- [lowering] Allow empty sources ([#572](https://github.com/paradigmxyz/solar/issues/572))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Features

- Configurable logging destination ([#498](https://github.com/paradigmxyz/solar/issues/498))

### Miscellaneous Tasks

- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Features

- [interface] Impl Default for Session, create dcx from opts ([#471](https://github.com/paradigmxyz/solar/issues/471))

### Miscellaneous Tasks

- Update signal handler ([#463](https://github.com/paradigmxyz/solar/issues/463))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Features

- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Rename enter to enter_sequential ([#392](https://github.com/paradigmxyz/solar/issues/392))

### Performance

- Load input source files in parallel ([#429](https://github.com/paradigmxyz/solar/issues/429))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Dependencies

- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))

## [Unreleased](https://github.com/paradigmxyz/solar/compare/v0.1.3...HEAD)

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Features

- `--no-warnings` ([#293](https://github.com/paradigmxyz/solar/issues/293))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))

### Miscellaneous Tasks

- Add some more non_exhaustive ([#261](https://github.com/paradigmxyz/solar/issues/261))
- Revert extra changelog

### Performance

- Add and default to using mimalloc ([#276](https://github.com/paradigmxyz/solar/issues/276))

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Correct features in version string ([#242](https://github.com/paradigmxyz/solar/issues/242))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Allow compiling out tracing-subscriber ([#213](https://github.com/paradigmxyz/solar/issues/213))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Add Session::enter_parallel ([#183](https://github.com/paradigmxyz/solar/issues/183))
- Install rayon pool in Session::enter ([#123](https://github.com/paradigmxyz/solar/issues/123))

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))
- Add more methods to index types ([#156](https://github.com/paradigmxyz/solar/issues/156))
- Print AST statistics with -Zast-stats flag ([#125](https://github.com/paradigmxyz/solar/issues/125))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
