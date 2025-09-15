# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Features

- [interface] Add Session::reconfigure ([#491](https://github.com/paradigmxyz/solar/issues/491))
- Diagnostic suggestions ([#474](https://github.com/paradigmxyz/solar/issues/474))
- Add error format options for human-readable diagnostics ([#473](https://github.com/paradigmxyz/solar/issues/473))
- [interface] Impl Default for Session, create dcx from opts ([#471](https://github.com/paradigmxyz/solar/issues/471))
- Bump to annotate-snippets 0.12, diagnostic tweaks ([#465](https://github.com/paradigmxyz/solar/issues/465))
- Add another utility method for extracting diagnostics ([#455](https://github.com/paradigmxyz/solar/issues/455))
- [diagnostics] Track notes + expose notes/warn counts ([#447](https://github.com/paradigmxyz/solar/issues/447))
- `InMemoryEmitter` ([#451](https://github.com/paradigmxyz/solar/issues/451))

### Miscellaneous Tasks

- Improve 'parsed' debug log ([#489](https://github.com/paradigmxyz/solar/issues/489))
- [interface] Rename dcx flag setters ([#478](https://github.com/paradigmxyz/solar/issues/478))
- Move VERSION to config::version::SEMVER_VERSION and log it ([#454](https://github.com/paradigmxyz/solar/issues/454))
- Hide more source map implementation details ([#450](https://github.com/paradigmxyz/solar/issues/450))
- Chore!(data-structures): remove aliases in sync re-exports ([#452](https://github.com/paradigmxyz/solar/issues/452))
- Remove deprecated items ([#449](https://github.com/paradigmxyz/solar/issues/449))
- [meta] Update solidity links ([#448](https://github.com/paradigmxyz/solar/issues/448))

### Performance

- Diagnostic suggestions ([#483](https://github.com/paradigmxyz/solar/issues/483))
- [lexer] Minor improvements ([#480](https://github.com/paradigmxyz/solar/issues/480))
- [interface] Cache thread pool inside of session ([#458](https://github.com/paradigmxyz/solar/issues/458))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Bug Fixes

- Don't print fs errors twice ([#440](https://github.com/paradigmxyz/solar/issues/440))

### Features

- Add getters for source by file name (path) ([#442](https://github.com/paradigmxyz/solar/issues/442))
- [interface] Add FileLoader abstraction for fs/io ([#438](https://github.com/paradigmxyz/solar/issues/438))
- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Make `Lit`erals implement `Copy` ([#414](https://github.com/paradigmxyz/solar/issues/414))
- Add ByteSymbol, use in LitKind::Str ([#425](https://github.com/paradigmxyz/solar/issues/425))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Update analyze_source_file ([#430](https://github.com/paradigmxyz/solar/issues/430))
- Update docs, fix ci ([#403](https://github.com/paradigmxyz/solar/issues/403))
- Rename enter to enter_sequential ([#392](https://github.com/paradigmxyz/solar/issues/392))

### Performance

- Load input source files in parallel ([#429](https://github.com/paradigmxyz/solar/issues/429))
- Tweak inlining ([#426](https://github.com/paradigmxyz/solar/issues/426))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Dependencies

- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))

### Features

- Flatten diag msg with style ([#368](https://github.com/paradigmxyz/solar/issues/368))

### Miscellaneous Tasks

- Store SessionGlobals inside of Session ([#379](https://github.com/paradigmxyz/solar/issues/379))
- Use Option<StateMutability> in the AST ([#374](https://github.com/paradigmxyz/solar/issues/374))
- Fn header spans ([#371](https://github.com/paradigmxyz/solar/issues/371))
- Clippy

### Performance

- Use `inturn` as the interner ([#349](https://github.com/paradigmxyz/solar/issues/349))

## [0.1.4](https://github.com/paradigmxyz/solar/releases/tag/v0.1.4)

### Bug Fixes

- Try absolute paths in file resolver too ([#323](https://github.com/paradigmxyz/solar/issues/323))

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Bug Fixes

- [parser] Don't panic when parsing hex rationals ([#256](https://github.com/paradigmxyz/solar/issues/256))

### Features

- Allow constructing DiagId with a string ([#295](https://github.com/paradigmxyz/solar/issues/295))
- Warn when payable fallback function found but no receive function ([#170](https://github.com/paradigmxyz/solar/issues/170))
- Pretty printing utilities ([#264](https://github.com/paradigmxyz/solar/issues/264))

### Miscellaneous Tasks

- Rust 1.86 ([#290](https://github.com/paradigmxyz/solar/issues/290))
- Update hint fns ([#289](https://github.com/paradigmxyz/solar/issues/289))
- Misc map cleanups ([#277](https://github.com/paradigmxyz/solar/issues/277))
- Add some more non_exhaustive ([#261](https://github.com/paradigmxyz/solar/issues/261))
- Tweak tracing events ([#255](https://github.com/paradigmxyz/solar/issues/255))
- Sync sourcemap from upstream ([#254](https://github.com/paradigmxyz/solar/issues/254))
- Revert extra changelog

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Dedup resolved files ([#246](https://github.com/paradigmxyz/solar/issues/246))
- Panic in multiline diagnostics ([#193](https://github.com/paradigmxyz/solar/issues/193))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Refactor FileResolver to allow custom current_dir ([#235](https://github.com/paradigmxyz/solar/issues/235))
- Improve SourceMap helpers for Span to source ([#233](https://github.com/paradigmxyz/solar/issues/233))
- Parse storage layout specifiers ([#232](https://github.com/paradigmxyz/solar/issues/232))
- Make current_dir configurable ([#231](https://github.com/paradigmxyz/solar/issues/231))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))
- Implement minimal HIR visitor ([#195](https://github.com/paradigmxyz/solar/issues/195))

### Miscellaneous Tasks

- Make SilentEmitter wrap DynEmitter ([#199](https://github.com/paradigmxyz/solar/issues/199))
- Shorten Diagnostic* to Diag ([#184](https://github.com/paradigmxyz/solar/issues/184))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Add Session::enter_parallel ([#183](https://github.com/paradigmxyz/solar/issues/183))
- [parser] Accept leading dot in literals ([#151](https://github.com/paradigmxyz/solar/issues/151))
- Properly handle recursive types ([#133](https://github.com/paradigmxyz/solar/issues/133))
- Install rayon pool in Session::enter ([#123](https://github.com/paradigmxyz/solar/issues/123))

### Dependencies

- Update dependencies ([#161](https://github.com/paradigmxyz/solar/issues/161))

### Documentation

- Fix typos
- Add some more docs to Session ([#155](https://github.com/paradigmxyz/solar/issues/155))
- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Add some more Span utils ([#179](https://github.com/paradigmxyz/solar/issues/179))
- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))
- Add more methods to index types ([#156](https://github.com/paradigmxyz/solar/issues/156))
- Update to Rust 1.83 ([#150](https://github.com/paradigmxyz/solar/issues/150))
- Add some FileName functions ([#143](https://github.com/paradigmxyz/solar/issues/143))
- Print AST statistics with -Zast-stats flag ([#125](https://github.com/paradigmxyz/solar/issues/125))

### Miscellaneous Tasks

- Extend rayon threadpool comment ([#138](https://github.com/paradigmxyz/solar/issues/138))
- Remove Pos trait ([#137](https://github.com/paradigmxyz/solar/issues/137))

### Testing

- Add a test for SessionGlobals + Session::enter ([#142](https://github.com/paradigmxyz/solar/issues/142))
- Add another Session test ([#134](https://github.com/paradigmxyz/solar/issues/134))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
