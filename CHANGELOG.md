# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
