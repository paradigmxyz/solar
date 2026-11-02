# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Bug Fixes

- [sema] Drop cached output allocations again ([#1889](https://github.com/paradigmxyz/solar/issues/1889))
- [sema] Report direct struct cycles next to arrays ([#1819](https://github.com/paradigmxyz/solar/issues/1819))
- [parser] Raise the recursion limit to 256 ([#1704](https://github.com/paradigmxyz/solar/issues/1704))
- [sema] Drop cached output allocations ([#1349](https://github.com/paradigmxyz/solar/issues/1349))
- [yul] Complete EVM version checks
- [sema] Downgrade NatSpec errors to warnings ([#1281](https://github.com/paradigmxyz/solar/issues/1281))
- [docs] Avoid rustdoc ICE on re-exports ([#912](https://github.com/paradigmxyz/solar/issues/912))
- Align import resolution with solc ([#860](https://github.com/paradigmxyz/solar/issues/860))

### Features

- [test] Add external foundry testing infra ([#1213](https://github.com/paradigmxyz/solar/issues/1213))
- [codegen] Print pass diffs ([#1001](https://github.com/paradigmxyz/solar/issues/1001))
- Update solidity to 0.8.35 ([#874](https://github.com/paradigmxyz/solar/issues/874))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))

### Miscellaneous Tasks

- Rm allow ([#1283](https://github.com/paradigmxyz/solar/issues/1283))
- Release 0.2.0

### Performance

- [evm] Retain several constants on the stack ([#2122](https://github.com/paradigmxyz/solar/issues/2122))
- [codegen] Plan function stack layouts in parallel ([#2007](https://github.com/paradigmxyz/solar/issues/2007))
- [codegen] Speed up analyses and spill planning ([#1606](https://github.com/paradigmxyz/solar/issues/1606))
- [codegen] Speed up MIR and EVM IR passes ([#1556](https://github.com/paradigmxyz/solar/issues/1556))
- [codegen] Improve EVM stack scheduling ([#1305](https://github.com/paradigmxyz/solar/issues/1305))
- [codegen] Use dense sets for IDs ([#958](https://github.com/paradigmxyz/solar/issues/958))

### Refactor

- [data-structures] Re-export std::fmt::from_fn ([#1617](https://github.com/paradigmxyz/solar/issues/1617))
- Share bitset word operations ([#1607](https://github.com/paradigmxyz/solar/issues/1607))
- Use mod.rs for nested modules ([#979](https://github.com/paradigmxyz/solar/issues/979))
- Add shared bitset data structures ([#865](https://github.com/paradigmxyz/solar/issues/865))
- Replace index_vec with oxc_index ([#742](https://github.com/paradigmxyz/solar/issues/742))
- [interface] Update annotation_snippets emitter ([#602](https://github.com/paradigmxyz/solar/issues/602))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Bug Fixes

- OnDrop drops, rename to DropGuard ([#399](https://github.com/paradigmxyz/solar/issues/399))

### Features

- [data-structures] Add ThinSlice ([#545](https://github.com/paradigmxyz/solar/issues/545))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Release 0.1.8 ([#551](https://github.com/paradigmxyz/solar/issues/551))
- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))
- Release 0.1.7 ([#492](https://github.com/paradigmxyz/solar/issues/492))
- Chore!(data-structures): remove aliases in sync re-exports ([#452](https://github.com/paradigmxyz/solar/issues/452))
- Remove deprecated items ([#449](https://github.com/paradigmxyz/solar/issues/449))
- Release 0.1.6 ([#443](https://github.com/paradigmxyz/solar/issues/443))
- Add some more `#[track_caller]` ([#409](https://github.com/paradigmxyz/solar/issues/409))
- Update docs, fix ci ([#403](https://github.com/paradigmxyz/solar/issues/403))

### Performance

- [ast] Use ThinSlice ([#546](https://github.com/paradigmxyz/solar/issues/546))
- Implement likely/unlikely with `#[cold]` ([#386](https://github.com/paradigmxyz/solar/issues/386))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Dependencies

- [lexer] Rewrite prefixed literal lexing ([#325](https://github.com/paradigmxyz/solar/issues/325))
- [deps] Bump breaking deps ([#253](https://github.com/paradigmxyz/solar/issues/253))

### Documentation

- Fix `newtype_index!` generated doc comments ([#288](https://github.com/paradigmxyz/solar/issues/288))

### Features

- [sema] Display more types, add Ty::display ([#328](https://github.com/paradigmxyz/solar/issues/328))

### Miscellaneous Tasks

- Release 0.1.5 ([#382](https://github.com/paradigmxyz/solar/issues/382))
- Release 0.1.4 ([#341](https://github.com/paradigmxyz/solar/issues/341))
- Release 0.1.3 ([#310](https://github.com/paradigmxyz/solar/issues/310))
- Update hint fns ([#289](https://github.com/paradigmxyz/solar/issues/289))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Properly handle recursive types ([#133](https://github.com/paradigmxyz/solar/issues/133))

### Dependencies

- Update dependencies ([#161](https://github.com/paradigmxyz/solar/issues/161))

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Add more methods to index types ([#156](https://github.com/paradigmxyz/solar/issues/156))
- Add try_new to newtype_index! types ([#152](https://github.com/paradigmxyz/solar/issues/152))
- Validate variable data locations ([#149](https://github.com/paradigmxyz/solar/issues/149))

### Miscellaneous Tasks

- Release 0.1.1 ([#182](https://github.com/paradigmxyz/solar/issues/182))

## [0.2.0](https://github.com/paradigmxyz/solar/releases/tag/v0.2.0)

### Bug Fixes

- [docs] Avoid rustdoc ICE on re-exports ([#912](https://github.com/paradigmxyz/solar/issues/912))
- Align import resolution with solc ([#860](https://github.com/paradigmxyz/solar/issues/860))

### Features

- Update solidity to 0.8.35 ([#874](https://github.com/paradigmxyz/solar/issues/874))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))

### Refactor

- Add shared bitset data structures ([#865](https://github.com/paradigmxyz/solar/issues/865))
- Replace index_vec with oxc_index ([#742](https://github.com/paradigmxyz/solar/issues/742))
- [interface] Update annotation_snippets emitter ([#602](https://github.com/paradigmxyz/solar/issues/602))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Features

- [data-structures] Add ThinSlice ([#545](https://github.com/paradigmxyz/solar/issues/545))

### Miscellaneous Tasks

- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))

### Performance

- [ast] Use ThinSlice ([#546](https://github.com/paradigmxyz/solar/issues/546))

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Miscellaneous Tasks

- Chore!(data-structures): remove aliases in sync re-exports ([#452](https://github.com/paradigmxyz/solar/issues/452))
- Remove deprecated items ([#449](https://github.com/paradigmxyz/solar/issues/449))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Bug Fixes

- OnDrop drops, rename to DropGuard ([#399](https://github.com/paradigmxyz/solar/issues/399))

### Features

- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Add some more `#[track_caller]` ([#409](https://github.com/paradigmxyz/solar/issues/409))
- Update docs, fix ci ([#403](https://github.com/paradigmxyz/solar/issues/403))

### Performance

- Implement likely/unlikely with `#[cold]` ([#386](https://github.com/paradigmxyz/solar/issues/386))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Dependencies

- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))

## [0.1.4](https://github.com/paradigmxyz/solar/releases/tag/v0.1.4)

### Dependencies

- [lexer] Rewrite prefixed literal lexing ([#325](https://github.com/paradigmxyz/solar/issues/325))

### Features

- [sema] Display more types, add Ty::display ([#328](https://github.com/paradigmxyz/solar/issues/328))

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Dependencies

- [deps] Bump breaking deps ([#253](https://github.com/paradigmxyz/solar/issues/253))

### Documentation

- Fix `newtype_index!` generated doc comments ([#288](https://github.com/paradigmxyz/solar/issues/288))

### Miscellaneous Tasks

- Update hint fns ([#289](https://github.com/paradigmxyz/solar/issues/289))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Properly handle recursive types ([#133](https://github.com/paradigmxyz/solar/issues/133))

### Dependencies

- Update dependencies ([#161](https://github.com/paradigmxyz/solar/issues/161))

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Add more methods to index types ([#156](https://github.com/paradigmxyz/solar/issues/156))
- Add try_new to newtype_index! types ([#152](https://github.com/paradigmxyz/solar/issues/152))
- Validate variable data locations ([#149](https://github.com/paradigmxyz/solar/issues/149))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
