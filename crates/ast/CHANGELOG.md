# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Bug Fixes

- [ast] Debug for Token ([#512](https://github.com/paradigmxyz/solar/issues/512))
- [ast] Store yul::Expr even if only Call is allowed ([#496](https://github.com/paradigmxyz/solar/issues/496))
- [sema] Peel parens when lowering call args ([#495](https://github.com/paradigmxyz/solar/issues/495))

### Features

- [ast] Spanned optional commasep elements ([#543](https://github.com/paradigmxyz/solar/issues/543))

### Miscellaneous Tasks

- Add some traits to AstPath ([#549](https://github.com/paradigmxyz/solar/issues/549))
- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))

### Performance

- [ast] Use ThinSlice ([#546](https://github.com/paradigmxyz/solar/issues/546))
- [parser] General improvements ([#516](https://github.com/paradigmxyz/solar/issues/516))
- [parser] Pass Token in registers ([#509](https://github.com/paradigmxyz/solar/issues/509))
- [lexer] Avoid thread locals when we have a Session ([#507](https://github.com/paradigmxyz/solar/issues/507))

### Refactor

- [ast] Boxed `yul::StmtKind::For` to reduce the size of `yul::Stmt` ([#500](https://github.com/paradigmxyz/solar/issues/500))

### Testing

- Track node sizes ([#497](https://github.com/paradigmxyz/solar/issues/497))

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Dependencies

- [lexer] Inline token glueing into Cursor ([#479](https://github.com/paradigmxyz/solar/issues/479))

### Miscellaneous Tasks

- [meta] Update solidity links ([#448](https://github.com/paradigmxyz/solar/issues/448))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Bug Fixes

- [ast] Visit array size ([#437](https://github.com/paradigmxyz/solar/issues/437))

### Features

- Make `Lit`erals implement `Copy` ([#414](https://github.com/paradigmxyz/solar/issues/414))
- Add ByteSymbol, use in LitKind::Str ([#425](https://github.com/paradigmxyz/solar/issues/425))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))
- [sema] Add helper methods to Function ([#385](https://github.com/paradigmxyz/solar/issues/385))
- Visit_override when walking fn ([#383](https://github.com/paradigmxyz/solar/issues/383))

### Miscellaneous Tasks

- Update docs, fix ci ([#403](https://github.com/paradigmxyz/solar/issues/403))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Dependencies

- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))

### Features

- Resolve ctor base args ([#322](https://github.com/paradigmxyz/solar/issues/322))

### Miscellaneous Tasks

- Use Option<StateMutability> in the AST ([#374](https://github.com/paradigmxyz/solar/issues/374))
- Fn header spans ([#371](https://github.com/paradigmxyz/solar/issues/371))
- Clippy
- Misc cleanup, util methods ([#367](https://github.com/paradigmxyz/solar/issues/367))
- Add span to `TryCatchClause` ([#364](https://github.com/paradigmxyz/solar/issues/364))

## [0.1.4](https://github.com/paradigmxyz/solar/releases/tag/v0.1.4)

### Dependencies

- [lexer] Rewrite prefixed literal lexing ([#325](https://github.com/paradigmxyz/solar/issues/325))

### Features

- Add span in FunctionHeader ([#318](https://github.com/paradigmxyz/solar/issues/318))
- [ast] Add spans to blocks ([#314](https://github.com/paradigmxyz/solar/issues/314))

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Bug Fixes

- [parser] Named call arguments are allowed to be empty ([#283](https://github.com/paradigmxyz/solar/issues/283))
- [ast] BinOpToken::Caret is BinOpKind::BitXor ([#262](https://github.com/paradigmxyz/solar/issues/262))

### Features

- [ast] Add helpers to Delimiter ([#296](https://github.com/paradigmxyz/solar/issues/296))
- Allow constructing DiagId with a string ([#295](https://github.com/paradigmxyz/solar/issues/295))
- [ast] Store concatenated string literals ([#266](https://github.com/paradigmxyz/solar/issues/266))
- [ast] Add span to CallArgs ([#265](https://github.com/paradigmxyz/solar/issues/265))
- Pretty printing utilities ([#264](https://github.com/paradigmxyz/solar/issues/264))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))

### Miscellaneous Tasks

- Shorter Debug impls ([#291](https://github.com/paradigmxyz/solar/issues/291))
- Tweak tracing events ([#255](https://github.com/paradigmxyz/solar/issues/255))
- Revert extra changelog

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- [parser] Glob imports require an alias ([#245](https://github.com/paradigmxyz/solar/issues/245))
- Correctly resolve try/catch scopes ([#200](https://github.com/paradigmxyz/solar/issues/200))

### Dependencies

- [deps] Weekly `cargo update` ([#225](https://github.com/paradigmxyz/solar/issues/225))

### Features

- Parse storage layout specifiers ([#232](https://github.com/paradigmxyz/solar/issues/232))

### Miscellaneous Tasks

- [ast] Don't debug raw bytes in LitKind ([#236](https://github.com/paradigmxyz/solar/issues/236))

### Performance

- Make Token implement Copy ([#241](https://github.com/paradigmxyz/solar/issues/241))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- [parser] Accept leading dot in literals ([#151](https://github.com/paradigmxyz/solar/issues/151))

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Add some more Span utils ([#179](https://github.com/paradigmxyz/solar/issues/179))
- Set up codspeed ([#167](https://github.com/paradigmxyz/solar/issues/167))
- Syntax checker for functions with modifiers ([#164](https://github.com/paradigmxyz/solar/issues/164))
- [parser] Allow doc-comments anywhere ([#154](https://github.com/paradigmxyz/solar/issues/154))
- Validate variable data locations ([#149](https://github.com/paradigmxyz/solar/issues/149))
- Add some methods to CallArgs ([#140](https://github.com/paradigmxyz/solar/issues/140))
- [parser] Recover old-style fallbacks ([#135](https://github.com/paradigmxyz/solar/issues/135))
- Return ControlFlow from AST visitor methods ([#115](https://github.com/paradigmxyz/solar/issues/115))
- Add more semver compat ([#113](https://github.com/paradigmxyz/solar/issues/113))

### Miscellaneous Tasks

- [macros] Fix expansion spans ([#175](https://github.com/paradigmxyz/solar/issues/175))

### Refactor

- Split Ty printers ([#144](https://github.com/paradigmxyz/solar/issues/144))
- Re-export ast::* internal module ([#141](https://github.com/paradigmxyz/solar/issues/141))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
