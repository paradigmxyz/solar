# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Features

- [interface] Add FileLoader abstraction for fs/io ([#438](https://github.com/paradigmxyz/solar/issues/438))
- Make `Lit`erals implement `Copy` ([#414](https://github.com/paradigmxyz/solar/issues/414))
- Add ByteSymbol, use in LitKind::Str ([#425](https://github.com/paradigmxyz/solar/issues/425))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Miscellaneous Tasks

- Downgrade some debug spans to trace ([#412](https://github.com/paradigmxyz/solar/issues/412))
- Update docs, fix ci ([#403](https://github.com/paradigmxyz/solar/issues/403))

### Other

- Enforce typos ([#423](https://github.com/paradigmxyz/solar/issues/423))

### Performance

- [sema] Better parallel parser scheduling ([#428](https://github.com/paradigmxyz/solar/issues/428))
- [parser] Improve parse_lit for integers ([#427](https://github.com/paradigmxyz/solar/issues/427))
- Tweak inlining ([#426](https://github.com/paradigmxyz/solar/issues/426))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Dependencies

- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))

### Features

- [lexer] Add Cursor::with_position ([#348](https://github.com/paradigmxyz/solar/issues/348))

### Miscellaneous Tasks

- Store SessionGlobals inside of Session ([#379](https://github.com/paradigmxyz/solar/issues/379))
- Use Option<StateMutability> in the AST ([#374](https://github.com/paradigmxyz/solar/issues/374))
- Fn header spans ([#371](https://github.com/paradigmxyz/solar/issues/371))
- Clippy
- Add span to `TryCatchClause` ([#364](https://github.com/paradigmxyz/solar/issues/364))
- [parser] Move unescaping from lexer to parser ([#360](https://github.com/paradigmxyz/solar/issues/360))

### Performance

- [lexer] Optimize `is_id_continue_byte` using bitwise operations ([#347](https://github.com/paradigmxyz/solar/issues/347))

### Refactor

- Remove redundant EoF check ([#366](https://github.com/paradigmxyz/solar/issues/366))

## [0.1.4](https://github.com/paradigmxyz/solar/releases/tag/v0.1.4)

### Bug Fixes

- Windows eol lexing ([#340](https://github.com/paradigmxyz/solar/issues/340))
- [parser] Allow EVM builtins to be present in yul paths ([#336](https://github.com/paradigmxyz/solar/issues/336))

### Dependencies

- [lexer] Rewrite prefixed literal lexing ([#325](https://github.com/paradigmxyz/solar/issues/325))

### Features

- [sema] Implement receive function checks ([#321](https://github.com/paradigmxyz/solar/issues/321))
- Add span in FunctionHeader ([#318](https://github.com/paradigmxyz/solar/issues/318))
- [ast] Add spans to blocks ([#314](https://github.com/paradigmxyz/solar/issues/314))

### Miscellaneous Tasks

- [lexer] Cursor cleanup ([#338](https://github.com/paradigmxyz/solar/issues/338))

### Performance

- [lexer] Use slice::Iter instead of Chars in Cursor ([#339](https://github.com/paradigmxyz/solar/issues/339))
- [lexer] Use memchr in block_comment ([#337](https://github.com/paradigmxyz/solar/issues/337))
- [lexer] Add eat_until ([#324](https://github.com/paradigmxyz/solar/issues/324))

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Bug Fixes

- [parser] Clear docs if not consumed immediatly ([#309](https://github.com/paradigmxyz/solar/issues/309))
- [parser] Named call arguments are allowed to be empty ([#283](https://github.com/paradigmxyz/solar/issues/283))
- Invalid underscores check ([#281](https://github.com/paradigmxyz/solar/issues/281))
- [parser] Typo in concatenated string literals
- [parser] Align number parsing with solc ([#272](https://github.com/paradigmxyz/solar/issues/272))
- [parser] Correct token precedence ([#271](https://github.com/paradigmxyz/solar/issues/271))
- [parser] Ignore comments in lookahead ([#267](https://github.com/paradigmxyz/solar/issues/267))
- [parser] Remove RawTokenKind::UnknownPrefix ([#260](https://github.com/paradigmxyz/solar/issues/260))
- [parser] Don't panic when parsing hex rationals ([#256](https://github.com/paradigmxyz/solar/issues/256))

### Features

- [parser] Accept non-checksummed addresses, to be validated later ([#285](https://github.com/paradigmxyz/solar/issues/285))
- [ast] Store concatenated string literals ([#266](https://github.com/paradigmxyz/solar/issues/266))
- [ast] Add span to CallArgs ([#265](https://github.com/paradigmxyz/solar/issues/265))
- Pretty printing utilities ([#264](https://github.com/paradigmxyz/solar/issues/264))
- [parser] Use impl Into<String> ([#258](https://github.com/paradigmxyz/solar/issues/258))

### Miscellaneous Tasks

- [lexer] Add tokens.capacity log
- Tweak tracing events ([#255](https://github.com/paradigmxyz/solar/issues/255))
- Revert extra changelog

### Performance

- [lexer] Deal with bytes instead of chars in Cursor ([#279](https://github.com/paradigmxyz/solar/issues/279))
- [lexer] Filter out comments again ([#269](https://github.com/paradigmxyz/solar/issues/269))
- [lexer] Double tokens capacity estimate ([#268](https://github.com/paradigmxyz/solar/issues/268))

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- [parser] Glob imports require an alias ([#245](https://github.com/paradigmxyz/solar/issues/245))
- Correctly resolve try/catch scopes ([#200](https://github.com/paradigmxyz/solar/issues/200))

### Features

- Refactor FileResolver to allow custom current_dir ([#235](https://github.com/paradigmxyz/solar/issues/235))
- Parse storage layout specifiers ([#232](https://github.com/paradigmxyz/solar/issues/232))

### Miscellaneous Tasks

- Shorten Diagnostic* to Diag ([#184](https://github.com/paradigmxyz/solar/issues/184))

### Performance

- Make Token implement Copy ([#241](https://github.com/paradigmxyz/solar/issues/241))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Add Session::enter_parallel ([#183](https://github.com/paradigmxyz/solar/issues/183))
- [parser] Accept leading dot in literals ([#151](https://github.com/paradigmxyz/solar/issues/151))
- [parser] Span of partially-parsed expressions ([#139](https://github.com/paradigmxyz/solar/issues/139))
- [parser] Ignore more doc comments ([#136](https://github.com/paradigmxyz/solar/issues/136))

### Documentation

- Add icons ([#109](https://github.com/paradigmxyz/solar/issues/109))

### Features

- Add some more Span utils ([#179](https://github.com/paradigmxyz/solar/issues/179))
- [parser] Allow doc-comments anywhere ([#154](https://github.com/paradigmxyz/solar/issues/154))
- Validate variable data locations ([#149](https://github.com/paradigmxyz/solar/issues/149))
- [parser] Recover old-style fallbacks ([#135](https://github.com/paradigmxyz/solar/issues/135))
- Make parse_semver_req public ([#114](https://github.com/paradigmxyz/solar/issues/114))

### Miscellaneous Tasks

- Remove Pos trait ([#137](https://github.com/paradigmxyz/solar/issues/137))

### Other

- Validate num. variants in enum declaration ([#120](https://github.com/paradigmxyz/solar/issues/120))
- Better error for struct without any fields ([#121](https://github.com/paradigmxyz/solar/issues/121))

### Refactor

- Re-export ast::* internal module ([#141](https://github.com/paradigmxyz/solar/issues/141))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
