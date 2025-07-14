# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Bug Fixes

- Visit modifiers in hir visitor ([#373](https://github.com/paradigmxyz/solar/issues/373))

### Dependencies

- [deps] Weekly `cargo update` ([#381](https://github.com/paradigmxyz/solar/issues/381))
- [deps] Weekly `cargo update` ([#376](https://github.com/paradigmxyz/solar/issues/376))
- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))
- [deps] Weekly `cargo update` ([#372](https://github.com/paradigmxyz/solar/issues/372))
- [deps] Weekly `cargo update` ([#365](https://github.com/paradigmxyz/solar/issues/365))
- Bump & remove mimalloc patch ([#361](https://github.com/paradigmxyz/solar/issues/361))
- [deps] Weekly `cargo update` ([#358](https://github.com/paradigmxyz/solar/issues/358))
- [deps] Weekly `cargo update` ([#351](https://github.com/paradigmxyz/solar/issues/351))
- [deps] Weekly `cargo update` ([#345](https://github.com/paradigmxyz/solar/issues/345))

### Documentation

- Add CLAUDE.md for Claude Code guidance ([#357](https://github.com/paradigmxyz/solar/issues/357))
- Recommend default-features=false in README.md

### Features

- Resolve ctor base args ([#322](https://github.com/paradigmxyz/solar/issues/322))
- Flatten diag msg with style ([#368](https://github.com/paradigmxyz/solar/issues/368))
- Add span visitor debug tool ([#355](https://github.com/paradigmxyz/solar/issues/355))
- [lexer] Add Cursor::with_position ([#348](https://github.com/paradigmxyz/solar/issues/348))

### Miscellaneous Tasks

- Store SessionGlobals inside of Session ([#379](https://github.com/paradigmxyz/solar/issues/379))
- [benches] Extract Session initialization to its own benchmark ([#380](https://github.com/paradigmxyz/solar/issues/380))
- [meta] Add .git-blame-ignore-revs
- Use Option<StateMutability> in the AST ([#374](https://github.com/paradigmxyz/solar/issues/374))
- Fn header spans ([#371](https://github.com/paradigmxyz/solar/issues/371))
- Clippy
- Misc cleanup, util methods ([#367](https://github.com/paradigmxyz/solar/issues/367))
- Add span to `TryCatchClause` ([#364](https://github.com/paradigmxyz/solar/issues/364))
- [parser] Move unescaping from lexer to parser ([#360](https://github.com/paradigmxyz/solar/issues/360))

### Other

- Add ds store ([#362](https://github.com/paradigmxyz/solar/issues/362))
- Rm bench concurrency

### Performance

- Use `inturn` as the interner ([#349](https://github.com/paradigmxyz/solar/issues/349))
- [lexer] Optimize `is_id_continue_byte` using bitwise operations ([#347](https://github.com/paradigmxyz/solar/issues/347))

### Refactor

- Remove redundant EoF check ([#366](https://github.com/paradigmxyz/solar/issues/366))

## [0.1.4](https://github.com/paradigmxyz/solar/releases/tag/v0.1.4)

### Bug Fixes

- Windows eol lexing ([#340](https://github.com/paradigmxyz/solar/issues/340))
- [parser] Allow EVM builtins to be present in yul paths ([#336](https://github.com/paradigmxyz/solar/issues/336))
- [sema] Don't warn 3628 if no interface functions ([#330](https://github.com/paradigmxyz/solar/issues/330))
- Try absolute paths in file resolver too ([#323](https://github.com/paradigmxyz/solar/issues/323))

### Dependencies

- [deps] Weekly `cargo update` ([#333](https://github.com/paradigmxyz/solar/issues/333))
- [lexer] Rewrite prefixed literal lexing ([#325](https://github.com/paradigmxyz/solar/issues/325))
- [deps] Weekly `cargo update` ([#320](https://github.com/paradigmxyz/solar/issues/320))
- [deps] Weekly `cargo update` ([#311](https://github.com/paradigmxyz/solar/issues/311))

### Documentation

- Remove outdated section in CONTRIBUTING.md

### Features

- [sema] Implement receive function checks ([#321](https://github.com/paradigmxyz/solar/issues/321))
- [sema] Display more types, add Ty::display ([#328](https://github.com/paradigmxyz/solar/issues/328))
- Add span in FunctionHeader ([#318](https://github.com/paradigmxyz/solar/issues/318))
- [ast] Add spans to blocks ([#314](https://github.com/paradigmxyz/solar/issues/314))
- Typecheck for external type clashes ([#312](https://github.com/paradigmxyz/solar/issues/312))

### Miscellaneous Tasks

- [lexer] Cursor cleanup ([#338](https://github.com/paradigmxyz/solar/issues/338))
- [benches] Add Optimism
- [benches] Add Solady
- [meta] Fix deny.toml

### Other

- Remove concurrency from bench

### Performance

- [lexer] Use slice::Iter instead of Chars in Cursor ([#339](https://github.com/paradigmxyz/solar/issues/339))
- [lexer] Use memchr in block_comment ([#337](https://github.com/paradigmxyz/solar/issues/337))
- [lexer] Add eat_until ([#324](https://github.com/paradigmxyz/solar/issues/324))

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Bug Fixes

- [parser] Clear docs if not consumed immediatly ([#309](https://github.com/paradigmxyz/solar/issues/309))
- [sema] Declare selfdestruct builtin ([#297](https://github.com/paradigmxyz/solar/issues/297))
- [sema] Correct variable declaration scopes ([#300](https://github.com/paradigmxyz/solar/issues/300))
- [parser] Named call arguments are allowed to be empty ([#283](https://github.com/paradigmxyz/solar/issues/283))
- Invalid underscores check ([#281](https://github.com/paradigmxyz/solar/issues/281))
- [parser] Typo in concatenated string literals
- [parser] Align number parsing with solc ([#272](https://github.com/paradigmxyz/solar/issues/272))
- [parser] Correct token precedence ([#271](https://github.com/paradigmxyz/solar/issues/271))
- [parser] Ignore comments in lookahead ([#267](https://github.com/paradigmxyz/solar/issues/267))
- [ast] BinOpToken::Caret is BinOpKind::BitXor ([#262](https://github.com/paradigmxyz/solar/issues/262))
- [parser] Remove RawTokenKind::UnknownPrefix ([#260](https://github.com/paradigmxyz/solar/issues/260))
- [parser] Don't panic when parsing hex rationals ([#256](https://github.com/paradigmxyz/solar/issues/256))

### Dependencies

- [deps] Weekly `cargo update` ([#306](https://github.com/paradigmxyz/solar/issues/306))
- [deps] Weekly `cargo update` ([#286](https://github.com/paradigmxyz/solar/issues/286))
- [deps] Weekly `cargo update` ([#278](https://github.com/paradigmxyz/solar/issues/278))
- Add update=none to submodules ([#270](https://github.com/paradigmxyz/solar/issues/270))
- [deps] Weekly `cargo update` ([#257](https://github.com/paradigmxyz/solar/issues/257))
- [deps] Bump breaking deps ([#253](https://github.com/paradigmxyz/solar/issues/253))

### Documentation

- [meta] Update RELEASE_CHECKLIST.md
- Fix `newtype_index!` generated doc comments ([#288](https://github.com/paradigmxyz/solar/issues/288))
- Update library usage in README
- Update release checklist

### Features

- Update solidity to 0.8.30 ([#307](https://github.com/paradigmxyz/solar/issues/307))
- [ast] Add helpers to Delimiter ([#296](https://github.com/paradigmxyz/solar/issues/296))
- Allow constructing DiagId with a string ([#295](https://github.com/paradigmxyz/solar/issues/295))
- Warn when payable fallback function found but no receive function ([#170](https://github.com/paradigmxyz/solar/issues/170))
- `--no-warnings` ([#293](https://github.com/paradigmxyz/solar/issues/293))
- Literal checks ([#287](https://github.com/paradigmxyz/solar/issues/287))
- [parser] Accept non-checksummed addresses, to be validated later ([#285](https://github.com/paradigmxyz/solar/issues/285))
- [ast] Store concatenated string literals ([#266](https://github.com/paradigmxyz/solar/issues/266))
- [ast] Add span to CallArgs ([#265](https://github.com/paradigmxyz/solar/issues/265))
- Pretty printing utilities ([#264](https://github.com/paradigmxyz/solar/issues/264))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))
- [parser] Use impl Into<String> ([#258](https://github.com/paradigmxyz/solar/issues/258))

### Miscellaneous Tasks

- [meta] Update dist
- [lexer] Add tokens.capacity log
- Use astral-sh/cargo-dist fork ([#292](https://github.com/paradigmxyz/solar/issues/292))
- Shorter Debug impls ([#291](https://github.com/paradigmxyz/solar/issues/291))
- Rust 1.86 ([#290](https://github.com/paradigmxyz/solar/issues/290))
- Update hint fns ([#289](https://github.com/paradigmxyz/solar/issues/289))
- [benches] Add seaport ([#282](https://github.com/paradigmxyz/solar/issues/282))
- Add linguist-vendored
- [benches] Add tree-sitter ([#280](https://github.com/paradigmxyz/solar/issues/280))
- Misc map cleanups ([#277](https://github.com/paradigmxyz/solar/issues/277))
- [benches] Update and plot data ([#274](https://github.com/paradigmxyz/solar/issues/274))
- Add some more non_exhaustive ([#261](https://github.com/paradigmxyz/solar/issues/261))
- Tweak tracing events ([#255](https://github.com/paradigmxyz/solar/issues/255))
- Sync sourcemap from upstream ([#254](https://github.com/paradigmxyz/solar/issues/254))
- Add 'fetchRecurseSubmodules = false' to solidity submodule
- Revert extra changelog

### Performance

- [lexer] Deal with bytes instead of chars in Cursor ([#279](https://github.com/paradigmxyz/solar/issues/279))
- Add and default to using mimalloc ([#276](https://github.com/paradigmxyz/solar/issues/276))
- [lexer] Filter out comments again ([#269](https://github.com/paradigmxyz/solar/issues/269))
- [lexer] Double tokens capacity estimate ([#268](https://github.com/paradigmxyz/solar/issues/268))

### Testing

- Add a test for global storage param ([#299](https://github.com/paradigmxyz/solar/issues/299))
- Add another try catch resolve test ([#298](https://github.com/paradigmxyz/solar/issues/298))

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
