# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Hir call visiting ([#250](https://github.com/paradigmxyz/solar/issues/250))
- Public transient assert ([#248](https://github.com/paradigmxyz/solar/issues/248))
- Dedup resolved files ([#246](https://github.com/paradigmxyz/solar/issues/246))
- [parser] Glob imports require an alias ([#245](https://github.com/paradigmxyz/solar/issues/245))
- Point resolution errors to the import string ([#244](https://github.com/paradigmxyz/solar/issues/244))
- Correct features in version string ([#242](https://github.com/paradigmxyz/solar/issues/242))
- Unsound transmute ([#239](https://github.com/paradigmxyz/solar/issues/239))
- Disable default features
- Variable getter functions are external ([#202](https://github.com/paradigmxyz/solar/issues/202))
- Error when func type has named return param  ([#173](https://github.com/paradigmxyz/solar/issues/173))
- Don't check placeholders in virtual modifiers ([#201](https://github.com/paradigmxyz/solar/issues/201))
- Correctly resolve try/catch scopes ([#200](https://github.com/paradigmxyz/solar/issues/200))
- Panic in multiline diagnostics ([#193](https://github.com/paradigmxyz/solar/issues/193))
- Use custom build profile in --version ([#192](https://github.com/paradigmxyz/solar/issues/192))
- Correctly evaluate public constants ([#187](https://github.com/paradigmxyz/solar/issues/187))
- Allow storage in modifiers ([#185](https://github.com/paradigmxyz/solar/issues/185))

### Dependencies

- [deps] Weekly `cargo update` ([#249](https://github.com/paradigmxyz/solar/issues/249))
- Unpin and bump ui_test to 0.29.2 ([#247](https://github.com/paradigmxyz/solar/issues/247))
- [deps] Weekly `cargo update` ([#240](https://github.com/paradigmxyz/solar/issues/240))
- [deps] Weekly `cargo update` ([#229](https://github.com/paradigmxyz/solar/issues/229))
- Bump solidity submodule to 0.8.29 ([#230](https://github.com/paradigmxyz/solar/issues/230))
- [deps] Weekly `cargo update` ([#228](https://github.com/paradigmxyz/solar/issues/228))
- [deps] Weekly `cargo update` ([#225](https://github.com/paradigmxyz/solar/issues/225))
- [deps] Weekly `cargo update` ([#214](https://github.com/paradigmxyz/solar/issues/214))
- [deps] Weekly `cargo update` ([#209](https://github.com/paradigmxyz/solar/issues/209))
- [deps] Weekly `cargo update` ([#208](https://github.com/paradigmxyz/solar/issues/208))
- [deps] Weekly `cargo update` ([#206](https://github.com/paradigmxyz/solar/issues/206))
- [deps] Weekly `cargo update` ([#198](https://github.com/paradigmxyz/solar/issues/198))
- [deps] Weekly `cargo update` ([#194](https://github.com/paradigmxyz/solar/issues/194))
- [deps] Weekly `cargo update` ([#191](https://github.com/paradigmxyz/solar/issues/191))
- [deps] Weekly `cargo update` ([#190](https://github.com/paradigmxyz/solar/issues/190))
- [deps] Weekly `cargo update` ([#186](https://github.com/paradigmxyz/solar/issues/186))
- Update dependencies.yml
- Update dependencies.yml

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Refactor FileResolver to allow custom current_dir ([#235](https://github.com/paradigmxyz/solar/issues/235))
- Improve SourceMap helpers for Span to source ([#233](https://github.com/paradigmxyz/solar/issues/233))
- [hir] Expose hir from parsing context ([#210](https://github.com/paradigmxyz/solar/issues/210))
- Parse storage layout specifiers ([#232](https://github.com/paradigmxyz/solar/issues/232))
- Make current_dir configurable ([#231](https://github.com/paradigmxyz/solar/issues/231))
- Allow compiling out tracing-subscriber ([#213](https://github.com/paradigmxyz/solar/issues/213))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))
- Implement minimal HIR visitor ([#195](https://github.com/paradigmxyz/solar/issues/195))

### Miscellaneous Tasks

- Update dist
- Release 0.1.2 ([#251](https://github.com/paradigmxyz/solar/issues/251))
- [ast] Don't debug raw bytes in LitKind ([#236](https://github.com/paradigmxyz/solar/issues/236))
- Make SilentEmitter wrap DynEmitter ([#199](https://github.com/paradigmxyz/solar/issues/199))
- Add dist-dbg profile
- Clippy
- Shorten Diagnostic* to Diag ([#184](https://github.com/paradigmxyz/solar/issues/184))

### Other

- Fix GITHUB_TOKEN
- Fix GITHUB_TOKEN
- Use reusable cargo update workflow ([#188](https://github.com/paradigmxyz/solar/issues/188))

### Performance

- Make Token implement Copy ([#241](https://github.com/paradigmxyz/solar/issues/241))

### Testing

- Update source delim matching ([#237](https://github.com/paradigmxyz/solar/issues/237))
- Use Result instead of Option in should_skip ([#204](https://github.com/paradigmxyz/solar/issues/204))

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Hir call visiting ([#250](https://github.com/paradigmxyz/solar/issues/250))
- Public transient assert ([#248](https://github.com/paradigmxyz/solar/issues/248))
- Dedup resolved files ([#246](https://github.com/paradigmxyz/solar/issues/246))
- [parser] Glob imports require an alias ([#245](https://github.com/paradigmxyz/solar/issues/245))
- Point resolution errors to the import string ([#244](https://github.com/paradigmxyz/solar/issues/244))
- Correct features in version string ([#242](https://github.com/paradigmxyz/solar/issues/242))
- Unsound transmute ([#239](https://github.com/paradigmxyz/solar/issues/239))
- Disable default features
- Variable getter functions are external ([#202](https://github.com/paradigmxyz/solar/issues/202))
- Error when func type has named return param  ([#173](https://github.com/paradigmxyz/solar/issues/173))
- Don't check placeholders in virtual modifiers ([#201](https://github.com/paradigmxyz/solar/issues/201))
- Correctly resolve try/catch scopes ([#200](https://github.com/paradigmxyz/solar/issues/200))
- Panic in multiline diagnostics ([#193](https://github.com/paradigmxyz/solar/issues/193))
- Use custom build profile in --version ([#192](https://github.com/paradigmxyz/solar/issues/192))
- Correctly evaluate public constants ([#187](https://github.com/paradigmxyz/solar/issues/187))
- Allow storage in modifiers ([#185](https://github.com/paradigmxyz/solar/issues/185))

### Dependencies

- Bump solidity submodule to 0.8.29 ([#230](https://github.com/paradigmxyz/solar/issues/230))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Refactor FileResolver to allow custom current_dir ([#235](https://github.com/paradigmxyz/solar/issues/235))
- Improve SourceMap helpers for Span to source ([#233](https://github.com/paradigmxyz/solar/issues/233))
- [hir] Expose hir from parsing context ([#210](https://github.com/paradigmxyz/solar/issues/210))
- Parse storage layout specifiers ([#232](https://github.com/paradigmxyz/solar/issues/232))
- Make current_dir configurable ([#231](https://github.com/paradigmxyz/solar/issues/231))
- Allow compiling out tracing-subscriber ([#213](https://github.com/paradigmxyz/solar/issues/213))
- Saner defaults for single threaded targets ([#212](https://github.com/paradigmxyz/solar/issues/212))
- Implement minimal HIR visitor ([#195](https://github.com/paradigmxyz/solar/issues/195))

### Miscellaneous Tasks

- [ast] Don't debug raw bytes in LitKind ([#236](https://github.com/paradigmxyz/solar/issues/236))
- Make SilentEmitter wrap DynEmitter ([#199](https://github.com/paradigmxyz/solar/issues/199))
- Add dist-dbg profile
- Shorten Diagnostic* to Diag ([#184](https://github.com/paradigmxyz/solar/issues/184))

### Other

- Use reusable cargo update workflow ([#188](https://github.com/paradigmxyz/solar/issues/188))

### Performance

- Make Token implement Copy ([#241](https://github.com/paradigmxyz/solar/issues/241))

### Testing

- Update source delim matching ([#237](https://github.com/paradigmxyz/solar/issues/237))
- Use Result instead of Option in should_skip ([#204](https://github.com/paradigmxyz/solar/issues/204))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

Notable and breaking changes (!):

- The parser now fully supports doc-comments in any position ([#154](https://github.com/paradigmxyz/solar/issues/154)). This was the last major feature needed to support the full Solidity grammar, as implemented in solc. The parser and AST are now considered feature-complete.
- Fixed some bugs in the parser
- Implemented some more syntax checks and validations

### Library

- (!) Return ControlFlow from AST visitor methods ([#115](https://github.com/paradigmxyz/solar/issues/115))
- (!) Remove Pos trait ([#137](https://github.com/paradigmxyz/solar/issues/137))
- (!) Re-export solar_ast::ast::* internal module ([#141](https://github.com/paradigmxyz/solar/issues/141))
- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))
  - `Session::builder`'s individual config option methods have been removed in favor of using `Args` directly.
- Install rayon pool in Session::enter ([#123](https://github.com/paradigmxyz/solar/issues/123))
  - Add Session::enter_parallel ([#183](https://github.com/paradigmxyz/solar/issues/183))
  - The session is now parallel by default; `enter` will behave the same, use `enter_parallel` to be able to make use of rayon inside of the closure.

---

All changes:

### Bug Fixes

- Add Session::enter_parallel ([#183](https://github.com/paradigmxyz/solar/issues/183))
- Display order in AST stats ([#180](https://github.com/paradigmxyz/solar/issues/180))
- Reduce width of subnode name in ast-stats ([#157](https://github.com/paradigmxyz/solar/issues/157))
- [parser] Accept leading dot in literals ([#151](https://github.com/paradigmxyz/solar/issues/151))
- Exclude arrays from mapping getter returns ([#148](https://github.com/paradigmxyz/solar/issues/148))
- [parser] Span of partially-parsed expressions ([#139](https://github.com/paradigmxyz/solar/issues/139))
- [parser] Ignore more doc comments ([#136](https://github.com/paradigmxyz/solar/issues/136))
- Properly handle recursive types ([#133](https://github.com/paradigmxyz/solar/issues/133))
- Validate placeholder is within modifier ([#132](https://github.com/paradigmxyz/solar/issues/132))
- Install rayon pool in Session::enter ([#123](https://github.com/paradigmxyz/solar/issues/123))

### Dependencies

- Enable dependencies.yml
- Update dependencies ([#161](https://github.com/paradigmxyz/solar/issues/161))

### Documentation

- Fix typos
- Update CONTRIBUTING.md
- Add note about codspeed
- Add some more docs to Session ([#155](https://github.com/paradigmxyz/solar/issues/155))
- Add telegram link
- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Add some more Span utils ([#179](https://github.com/paradigmxyz/solar/issues/179))
- Unify CLI and Session options ([#176](https://github.com/paradigmxyz/solar/issues/176))
- Check placeholders inside unchecked blocks ([#172](https://github.com/paradigmxyz/solar/issues/172))
- Library requirements syntax checker ([#168](https://github.com/paradigmxyz/solar/issues/168))
- Set up codspeed ([#167](https://github.com/paradigmxyz/solar/issues/167))
- Underscores and literals validation ([#165](https://github.com/paradigmxyz/solar/issues/165))
- Receive function validation ([#166](https://github.com/paradigmxyz/solar/issues/166))
- Func visibility checks for free functions ([#163](https://github.com/paradigmxyz/solar/issues/163))
- Syntax checker for functions with modifiers ([#164](https://github.com/paradigmxyz/solar/issues/164))
- Variable declaration statements are not allowed as the body of loop ([#158](https://github.com/paradigmxyz/solar/issues/158))
- Validate functions with no visibility specified ([#160](https://github.com/paradigmxyz/solar/issues/160))
- Modifier definitions must have a placeholder ([#159](https://github.com/paradigmxyz/solar/issues/159))
- Add more methods to index types ([#156](https://github.com/paradigmxyz/solar/issues/156))
- [parser] Allow doc-comments anywhere ([#154](https://github.com/paradigmxyz/solar/issues/154))
- Add try_new to newtype_index! types ([#152](https://github.com/paradigmxyz/solar/issues/152))
- Update to Rust 1.83 ([#150](https://github.com/paradigmxyz/solar/issues/150))
- Validate variable data locations ([#149](https://github.com/paradigmxyz/solar/issues/149))
- Make TyAbiPrinter public ([#145](https://github.com/paradigmxyz/solar/issues/145))
- Add some FileName functions ([#143](https://github.com/paradigmxyz/solar/issues/143))
- Add some methods to CallArgs ([#140](https://github.com/paradigmxyz/solar/issues/140))
- [parser] Recover old-style fallbacks ([#135](https://github.com/paradigmxyz/solar/issues/135))
- Print AST statistics with -Zast-stats flag ([#125](https://github.com/paradigmxyz/solar/issues/125))
- Return ControlFlow from AST visitor methods ([#115](https://github.com/paradigmxyz/solar/issues/115))
- Make parse_semver_req public ([#114](https://github.com/paradigmxyz/solar/issues/114))
- Add more semver compat ([#113](https://github.com/paradigmxyz/solar/issues/113))

### Miscellaneous Tasks

- Update process
- Update dist to 0.27.0
- Cargo update ([#181](https://github.com/paradigmxyz/solar/issues/181))
- [xtask] Bless = uibless
- [macros] Fix expansion spans ([#175](https://github.com/paradigmxyz/solar/issues/175))
- Add TyAbiPrinterMode ([#147](https://github.com/paradigmxyz/solar/issues/147))
- Mark TyKind as non_exhaustive ([#146](https://github.com/paradigmxyz/solar/issues/146))
- Extend rayon threadpool comment ([#138](https://github.com/paradigmxyz/solar/issues/138))
- Remove Pos trait ([#137](https://github.com/paradigmxyz/solar/issues/137))
- [meta] Add bug report template ([#131](https://github.com/paradigmxyz/solar/issues/131))
- Use unimplemented! instead of todo! in eval.rs ([#110](https://github.com/paradigmxyz/solar/issues/110))
- Fix deny.toml

### Other

- Move deny to ci ([#162](https://github.com/paradigmxyz/solar/issues/162))
- Initial AST validation for using-for ([#119](https://github.com/paradigmxyz/solar/issues/119))
- AST validate that a contract does not have a function with contract name ([#117](https://github.com/paradigmxyz/solar/issues/117))
- Validate num. variants in enum declaration ([#120](https://github.com/paradigmxyz/solar/issues/120))
- Better error for struct without any fields ([#121](https://github.com/paradigmxyz/solar/issues/121))
- AST validate unchecked nested blocks ([#116](https://github.com/paradigmxyz/solar/issues/116))
- AST validate check statement only in while/for loops ([#111](https://github.com/paradigmxyz/solar/issues/111))
- Add syntax test pragma solidity ([#112](https://github.com/paradigmxyz/solar/issues/112))

### Refactor

- Split Ty printers ([#144](https://github.com/paradigmxyz/solar/issues/144))
- Re-export ast::* internal module ([#141](https://github.com/paradigmxyz/solar/issues/141))

### Testing

- Add a test for SessionGlobals + Session::enter ([#142](https://github.com/paradigmxyz/solar/issues/142))
- Add another Session test ([#134](https://github.com/paradigmxyz/solar/issues/134))
- Add some more tests ([#122](https://github.com/paradigmxyz/solar/issues/122))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
