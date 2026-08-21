# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0](https://github.com/paradigmxyz/solar/releases/tag/v0.2.0)

### Bug Fixes

- [sema] Encode library function signatures like solc ([#926](https://github.com/paradigmxyz/solar/issues/926))
- [sema] Resolve public constant member over its getter ([#927](https://github.com/paradigmxyz/solar/issues/927))
- [release] Require crate changelogs ([#924](https://github.com/paradigmxyz/solar/issues/924))
- [release] Default vergen metadata on error ([#923](https://github.com/paradigmxyz/solar/issues/923))
- [config] Handle packaged version builds ([#922](https://github.com/paradigmxyz/solar/issues/922))
- [parser] Recover malformed member access ([#914](https://github.com/paradigmxyz/solar/issues/914))
- [docs] Avoid rustdoc ICE on re-exports ([#912](https://github.com/paradigmxyz/solar/issues/912))
- [lsp] Only scan open files without workspace ([#898](https://github.com/paradigmxyz/solar/issues/898))
- [parser] Clear error literal denominations ([#896](https://github.com/paradigmxyz/solar/issues/896))
- Avoid unused ui_test patch warning ([#894](https://github.com/paradigmxyz/solar/issues/894))
- [foundry] Clean up solc wrapper ([#889](https://github.com/paradigmxyz/solar/issues/889))
- [sema] Stabilize diagnostic order ([#879](https://github.com/paradigmxyz/solar/issues/879))
- Align import resolution with solc ([#860](https://github.com/paradigmxyz/solar/issues/860))
- [bench] Preserve single-threaded session ([#859](https://github.com/paradigmxyz/solar/issues/859))
- Add codes to warnings ([#839](https://github.com/paradigmxyz/solar/issues/839))
- [parse] Preserve hex escape bytes ([#831](https://github.com/paradigmxyz/solar/issues/831))
- [sema] Check variadic builtins ([#815](https://github.com/paradigmxyz/solar/issues/815))
- [sema] Allow struct NatSpec params ([#827](https://github.com/paradigmxyz/solar/issues/827))
- [sema] Inherit public variable getter docs ([#825](https://github.com/paradigmxyz/solar/issues/825))
- [sema] Expose inherited function selectors ([#824](https://github.com/paradigmxyz/solar/issues/824))
- [sema] Attach functions to function values ([#801](https://github.com/paradigmxyz/solar/issues/801))
- [sema] Expose contract type members ([#814](https://github.com/paradigmxyz/solar/issues/814))
- [sema] Reject oversized memory arrays ([#803](https://github.com/paradigmxyz/solar/issues/803))
- [sema] Expose contract scoped type members ([#793](https://github.com/paradigmxyz/solar/issues/793))
- [sema] Handle storage call lvalues ([#808](https://github.com/paradigmxyz/solar/issues/808))
- [sema] Allow tuple assignment holes ([#798](https://github.com/paradigmxyz/solar/issues/798))
- [sema] Validate storage layout base slots ([#810](https://github.com/paradigmxyz/solar/issues/810))
- [sema] Cast library names to address ([#804](https://github.com/paradigmxyz/solar/issues/804))
- [sema] Prefer most derived call target ([#800](https://github.com/paradigmxyz/solar/issues/800))
- [sema] Validate inline array literals ([#794](https://github.com/paradigmxyz/solar/issues/794))
- [sema] Index calldata slices ([#807](https://github.com/paradigmxyz/solar/issues/807))
- [sema] Type yul address literals as words ([#799](https://github.com/paradigmxyz/solar/issues/799))
- [sema] Bind storage array methods ([#802](https://github.com/paradigmxyz/solar/issues/802))
- [sema] Ignore index args for lvalues ([#796](https://github.com/paradigmxyz/solar/issues/796))
- [sema] Reject invalid using modifiers ([#790](https://github.com/paradigmxyz/solar/issues/790))
- [sema] Follow up function member kinds ([#788](https://github.com/paradigmxyz/solar/issues/788))
- [sema] Model function member kinds ([#786](https://github.com/paradigmxyz/solar/issues/786))
- [sema] Type yul string literals as words ([#785](https://github.com/paradigmxyz/solar/issues/785))
- [sema] Typeck yul ([#774](https://github.com/paradigmxyz/solar/issues/774))
- [typeck] Allow dynamic array new args ([#784](https://github.com/paradigmxyz/solar/issues/784))
- [typeck] Preserve mapping element locations ([#783](https://github.com/paradigmxyz/solar/issues/783))
- [typeck] Allow address literal casts ([#782](https://github.com/paradigmxyz/solar/issues/782))
- [typeck] Locate dynamic casts ([#781](https://github.com/paradigmxyz/solar/issues/781))
- [typeck] Allow implicit fixed bytes literals ([#780](https://github.com/paradigmxyz/solar/issues/780))
- [typeck] Allow immutable construction writes ([#778](https://github.com/paradigmxyz/solar/issues/778))
- [sema] Coerce private function pointers ([#776](https://github.com/paradigmxyz/solar/issues/776))
- [sema] Convert hex literals to fixed bytes ([#775](https://github.com/paradigmxyz/solar/issues/775))
- [typeck] Cap literal constant evaluation
- [sema] Align natspec validation ([#771](https://github.com/paradigmxyz/solar/issues/771))
- [interface] Honor no_warnings in DiagCtxtFlags::update_from_opts ([#744](https://github.com/paradigmxyz/solar/issues/744))
- [parser] Parse exponentiation right-assoc ([#767](https://github.com/paradigmxyz/solar/issues/767))
- [sema] Canonicalize bare integer aliases ([#761](https://github.com/paradigmxyz/solar/issues/761))
- Address clippy and fmt CI failures
- [typeck] Explicit type check for payable address ([#690](https://github.com/paradigmxyz/solar/issues/690))
- [bench] Skip useless tables and plots in benchmark output ([#676](https://github.com/paradigmxyz/solar/issues/676))
- [sema] Add handling for event/error failures ([#660](https://github.com/paradigmxyz/solar/issues/660))
- [typeck] Mark event/error typechecking todo ([#658](https://github.com/paradigmxyz/solar/issues/658))
- [parse] Ignore mid-line '@' in doc cmnts ([#597](https://github.com/paradigmxyz/solar/issues/597))
- Replace unnecessary repetitions of structure name ([#581](https://github.com/paradigmxyz/solar/issues/581))
- [interface] Strip prefix from import resolution context ([#577](https://github.com/paradigmxyz/solar/issues/577))
- [lowering] Bring `this` and `super` into scope for constructor inheritance ([#570](https://github.com/paradigmxyz/solar/issues/570))
- Do not break on close paren for struct ([#562](https://github.com/paradigmxyz/solar/issues/562))
- [parser] Neg. exp. empty check, disallow plus ([#561](https://github.com/paradigmxyz/solar/issues/561))

### Dependencies

- [deps] Bump the ci-weekly group with 4 updates ([#919](https://github.com/paradigmxyz/solar/issues/919))
- [deps] Bump the npm-weekly group in /editors/vscode with 2 updates ([#920](https://github.com/paradigmxyz/solar/issues/920))
- Bump inturn to 0.2.0 ([#916](https://github.com/paradigmxyz/solar/issues/916))
- [zed] Bump solidity grammar ([#908](https://github.com/paradigmxyz/solar/issues/908))
- [deps] Bump the ci-weekly group with 4 updates ([#904](https://github.com/paradigmxyz/solar/issues/904))
- [deps] Bump the npm-weekly group in /editors/vscode with 4 updates ([#905](https://github.com/paradigmxyz/solar/issues/905))
- Bump vergen to v10 ([#899](https://github.com/paradigmxyz/solar/issues/899))
- [deps] Weekly `cargo update` ([#891](https://github.com/paradigmxyz/solar/issues/891))
- [deps] Bump the ci-weekly group with 5 updates ([#876](https://github.com/paradigmxyz/solar/issues/876))
- [deps] Weekly `cargo update` ([#875](https://github.com/paradigmxyz/solar/issues/875))
- [deps] Weekly `cargo update` ([#851](https://github.com/paradigmxyz/solar/issues/851))
- [deps] Weekly `cargo update` ([#843](https://github.com/paradigmxyz/solar/issues/843))
- [deps] Weekly `cargo update` ([#832](https://github.com/paradigmxyz/solar/issues/832))
- [deps] Weekly `cargo update` ([#813](https://github.com/paradigmxyz/solar/issues/813))
- Bump MSRV ([#764](https://github.com/paradigmxyz/solar/issues/764))
- [deps] Weekly `cargo update` ([#738](https://github.com/paradigmxyz/solar/issues/738))
- [deps] Weekly `cargo update` ([#736](https://github.com/paradigmxyz/solar/issues/736))
- [deps] Weekly `cargo update` ([#732](https://github.com/paradigmxyz/solar/issues/732))
- [deps] Bump bytes from 1.11.0 to 1.11.1 ([#730](https://github.com/paradigmxyz/solar/issues/730))
- [deps] Weekly `cargo update` ([#729](https://github.com/paradigmxyz/solar/issues/729))
- Optimize non-critical deps for size ([#728](https://github.com/paradigmxyz/solar/issues/728))
- [deps] Weekly `cargo update` ([#725](https://github.com/paradigmxyz/solar/issues/725))
- [deps] Bump vergen to 10 ([#461](https://github.com/paradigmxyz/solar/issues/461))
- [deps] Weekly `cargo update` ([#720](https://github.com/paradigmxyz/solar/issues/720))
- Bump benches/analyze dependencies ([#724](https://github.com/paradigmxyz/solar/issues/724))
- [deps] Weekly `cargo update` ([#673](https://github.com/paradigmxyz/solar/issues/673))
- [deps] Run cargo shear ([#670](https://github.com/paradigmxyz/solar/issues/670))
- [deps] Weekly `cargo update` ([#668](https://github.com/paradigmxyz/solar/issues/668))
- [deps] Weekly `cargo update` ([#635](https://github.com/paradigmxyz/solar/issues/635))
-  test: bump solidity submodule to 0.8.31 ([#608](https://github.com/paradigmxyz/solar/issues/608))
- [deps] Weekly `cargo update` ([#607](https://github.com/paradigmxyz/solar/issues/607))
- [deps] Bump actions/checkout from 5 to 6 ([#604](https://github.com/paradigmxyz/solar/issues/604))
- [deps] Weekly `cargo update` ([#603](https://github.com/paradigmxyz/solar/issues/603))
- [deps] Weekly `cargo update` ([#601](https://github.com/paradigmxyz/solar/issues/601))
- [deps] Weekly `cargo update` ([#599](https://github.com/paradigmxyz/solar/issues/599))
- [deps] Bump actions/upload-artifact from 4 to 5 ([#589](https://github.com/paradigmxyz/solar/issues/589))
- [deps] Bump actions/download-artifact from 4 to 6 ([#590](https://github.com/paradigmxyz/solar/issues/590))
- [deps] Weekly `cargo update` ([#583](https://github.com/paradigmxyz/solar/issues/583))
- [deps] Weekly `cargo update` ([#582](https://github.com/paradigmxyz/solar/issues/582))
- [deps] Weekly `cargo update` ([#571](https://github.com/paradigmxyz/solar/issues/571))
- [deps] Weekly `cargo update` ([#558](https://github.com/paradigmxyz/solar/issues/558))

### Documentation

- Document FileCheck UI annotations
- Update SECURITY.md
- Update benchmarks for 0.1.8 ([#553](https://github.com/paradigmxyz/solar/issues/553))
- Typo in CHANGELOG.md ([#552](https://github.com/paradigmxyz/solar/issues/552))

### Features

- [lsp] Add inlay hints ([#918](https://github.com/paradigmxyz/solar/issues/918))
- [lsp] Add auto-completion support ([#915](https://github.com/paradigmxyz/solar/issues/915))
- Add solsmith and solreduce tools ([#911](https://github.com/paradigmxyz/solar/issues/911))
- [lsp] Add symbol navigation index ([#909](https://github.com/paradigmxyz/solar/issues/909))
- [lsp] Add symbol providers ([#906](https://github.com/paradigmxyz/solar/issues/906))
- [vscode] Add syntax highlighting ([#907](https://github.com/paradigmxyz/solar/issues/907))
- [hir] Add pretty printer ([#903](https://github.com/paradigmxyz/solar/issues/903))
- Add HIR stats flag ([#893](https://github.com/paradigmxyz/solar/issues/893))
- [lsp] Extract declaration symbol tables ([#888](https://github.com/paradigmxyz/solar/issues/888))
- Show AST enum variant sizes ([#892](https://github.com/paradigmxyz/solar/issues/892))
-   feat(lsp): load workspace config across lifecycle events ([#881](https://github.com/paradigmxyz/solar/issues/881))
- Add Fandango source code and runtime fuzzing ([#882](https://github.com/paradigmxyz/solar/issues/882))
- Port editor extensions to main repo ([#883](https://github.com/paradigmxyz/solar/issues/883))
- Introduce EVM IR ([#868](https://github.com/paradigmxyz/solar/issues/868))
- [lsp] Register watched file notifications ([#880](https://github.com/paradigmxyz/solar/issues/880))
- [lsp] Add initial language server support ([#870](https://github.com/paradigmxyz/solar/issues/870))
- Add erc7201 builtin ([#877](https://github.com/paradigmxyz/solar/issues/877))
- Update solidity to 0.8.35 ([#874](https://github.com/paradigmxyz/solar/issues/874))
- [codegen] Cost-model load PRE insertions ([#863](https://github.com/paradigmxyz/solar/issues/863))
- [codegen] Preserve stack across branch edges ([#862](https://github.com/paradigmxyz/solar/issues/862))
- Mem2reg improvements and evm codegen fixes ([#861](https://github.com/paradigmxyz/solar/issues/861))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))
- Add solc-compatible wasm API ([#850](https://github.com/paradigmxyz/solar/issues/850))
- Continue after frontend errors ([#838](https://github.com/paradigmxyz/solar/issues/838))
- Allow warnings by code ([#837](https://github.com/paradigmxyz/solar/issues/837))
- Improve parser/lowering ([#835](https://github.com/paradigmxyz/solar/issues/835))
- Expose expr types after typeck ([#833](https://github.com/paradigmxyz/solar/issues/833))
- [sema] Allow require custom errors ([#819](https://github.com/paradigmxyz/solar/issues/819))
- [cli] Support standard json pipeline ([#829](https://github.com/paradigmxyz/solar/issues/829))
- [sema] Add implicit tuple conversions ([#828](https://github.com/paradigmxyz/solar/issues/828))
- [parser] Add import callback ([#823](https://github.com/paradigmxyz/solar/issues/823))
- [sema] Implement using for ([#773](https://github.com/paradigmxyz/solar/issues/773))
- [typeck] Implement constant folding for integer literal type preservation ([#649](https://github.com/paradigmxyz/solar/issues/649))
- [sema] Lower inline yul to hir ([#769](https://github.com/paradigmxyz/solar/issues/769))
- [sema] Lower natspec ([#768](https://github.com/paradigmxyz/solar/issues/768))
- [sema] Implement override checker ([#685](https://github.com/paradigmxyz/solar/issues/685))
- SIMD optimizations, bug fixes, and struct codegen improvements
- [sema] Implement call type checking ([#717](https://github.com/paradigmxyz/solar/issues/717))
- [sema] Add internal function pointer check and improve check_assign ([#718](https://github.com/paradigmxyz/solar/issues/718))
- [sema] Implement variable declaration rules ([#681](https://github.com/paradigmxyz/solar/issues/681))
- [sema] Add implicit function pointer conversions ([#715](https://github.com/paradigmxyz/solar/issues/715))
- [sema] Add implicit tuple conversions ([#713](https://github.com/paradigmxyz/solar/issues/713))
- [sema] Add implicit fixed bytes conversions ([#712](https://github.com/paradigmxyz/solar/issues/712))
- [sema] Add implicit integer width conversions ([#711](https://github.com/paradigmxyz/solar/issues/711))
- [sema] Add bytes <-> string explicit conversions ([#708](https://github.com/paradigmxyz/solar/issues/708))
- [sema] Add function implementation checks ([#691](https://github.com/paradigmxyz/solar/issues/691))
- [sema] Integer explicit conversions ([#633](https://github.com/paradigmxyz/solar/issues/633))
- [sema] Implicit bytes literal conversion to bytes dynamic/fixed ([#642](https://github.com/paradigmxyz/solar/issues/642))
- [typeck] Support negative integer literal coercion  ([#648](https://github.com/paradigmxyz/solar/issues/648))
- [typeck] Implement implicit integer literal coercion ([#647](https://github.com/paradigmxyz/solar/issues/647))
- [ast] Change TypeSize to store bits instead of bytes ([#671](https://github.com/paradigmxyz/solar/issues/671))
- [typeck] Contracts/address explicit conversions ([#646](https://github.com/paradigmxyz/solar/issues/646))
- [cli] Add tracing-samply ([#669](https://github.com/paradigmxyz/solar/issues/669))
- [typeck] Contracts implicit conversions ([#634](https://github.com/paradigmxyz/solar/issues/634))
- [typeck] Check lvalue ([#641](https://github.com/paradigmxyz/solar/issues/641))
- [typeck,ast_lowering] Constructor base arguments validation ([#580](https://github.com/paradigmxyz/solar/issues/580))
- Fix mapping ICE + impl data location coercion ([#637](https://github.com/paradigmxyz/solar/issues/637))
- [sema] Explicit bytes conversion ([#632](https://github.com/paradigmxyz/solar/issues/632))
- [sema] Address explicit conversion ([#626](https://github.com/paradigmxyz/solar/issues/626))
- [sema] Implement explicit conversions for fixed-size byte arrays ([#624](https://github.com/paradigmxyz/solar/issues/624))
- Switch default emitter kind to unicode ([#629](https://github.com/paradigmxyz/solar/issues/629))
- [sema] Enum explicit conversion ([#625](https://github.com/paradigmxyz/solar/issues/625))
- [sema] Implement array slice implicit conversion ([#623](https://github.com/paradigmxyz/solar/issues/623))
- [typeck] Payable address implicit conversion ([#622](https://github.com/paradigmxyz/solar/issues/622))
- Implement HIR builder ([#559](https://github.com/paradigmxyz/solar/issues/559))
- [ast] Naive natspec ([#470](https://github.com/paradigmxyz/solar/issues/470))
- [sema] Experimental typeck ([#563](https://github.com/paradigmxyz/solar/issues/563))
- Complete hir visitor ([#557](https://github.com/paradigmxyz/solar/issues/557))
- [ast] Add some methods to BinOpKind, DataLocation ([#555](https://github.com/paradigmxyz/solar/issues/555))

### Miscellaneous Tasks

- Fix changelog script
- Update crossbeam-epoch
- Link zed rust-analyzer project ([#901](https://github.com/paradigmxyz/solar/issues/901))
- Improve AGENTS.md
- Group npm dependabot updates
- Update dependabot
- Rename benchmarks ([#857](https://github.com/paradigmxyz/solar/issues/857))
- Dep upgrades ([#852](https://github.com/paradigmxyz/solar/issues/852))
- [meta] Add .config/nextest.toml
- [meta] Stupid bot
- Migrate benchmarks to Gungraun ([#766](https://github.com/paradigmxyz/solar/issues/766))
- Tmp ignore ([#765](https://github.com/paradigmxyz/solar/issues/765))
- [meta] Remove RUSTFLAGS from AGENTS.md
- [meta] Ignore bincode advisory
- Display bits for `int_literal[n]` instead of bytes ([#672](https://github.com/paradigmxyz/solar/issues/672))
- [meta] Add fuzz directory ([#665](https://github.com/paradigmxyz/solar/issues/665))
- [sema] Remove todo ([#656](https://github.com/paradigmxyz/solar/issues/656))
- [meta] Compress AGENTS.md, add note about symbols ([#644](https://github.com/paradigmxyz/solar/issues/644))
- Sync annotate_snippets impl ([#630](https://github.com/paradigmxyz/solar/issues/630))
- [meta] Rename CLAUDE.md to AGENTS.md ([#628](https://github.com/paradigmxyz/solar/issues/628))
- [meta] Set issue types on issue templates ([#616](https://github.com/paradigmxyz/solar/issues/616))
- Warn instead err on invalid natspec tag ([#600](https://github.com/paradigmxyz/solar/issues/600))
- Disable clippy::test_attr_in_doctest ([#598](https://github.com/paradigmxyz/solar/issues/598))
- [interface] Expose semver MIN_SOLIDITY_VERSION ([#593](https://github.com/paradigmxyz/solar/issues/593))
- [meta] Add dependabot for GHA ([#587](https://github.com/paradigmxyz/solar/issues/587))
- [parse] Natspec w/ ws after '@' is valid ([#585](https://github.com/paradigmxyz/solar/issues/585))
- [lowering] Allow empty sources ([#572](https://github.com/paradigmxyz/solar/issues/572))
- [interface] Expose minimum supported solidity version ([#573](https://github.com/paradigmxyz/solar/issues/573))
- Update Cargo.lock
- Fix -Zdump path parser ([#554](https://github.com/paradigmxyz/solar/issues/554))

### Other

- Update to tempoxyz ([#721](https://github.com/paradigmxyz/solar/issues/721))
- Revert "feat: SIMD optimizations, bug fixes, and struct codegen improvements"
- Update permissions ([#591](https://github.com/paradigmxyz/solar/issues/591))
- Add codeql for GHA ([#588](https://github.com/paradigmxyz/solar/issues/588))
- Use `actions/checkout@v5` ([#565](https://github.com/paradigmxyz/solar/issues/565))

### Performance

- [sema] Skip unnecessary visitor walks ([#772](https://github.com/paradigmxyz/solar/issues/772))
- Use thin LTO for dist profile ([#727](https://github.com/paradigmxyz/solar/issues/727))

### Refactor

- [codegen] Pre-phases lowering cleanups ([#928](https://github.com/paradigmxyz/solar/issues/928))
- Print stats with comfy-table ([#897](https://github.com/paradigmxyz/solar/issues/897))
- [codegen] Deduplicate MIR helpers ([#864](https://github.com/paradigmxyz/solar/issues/864))
- Add shared bitset data structures ([#865](https://github.com/paradigmxyz/solar/issues/865))
- [evm] Clean up assembler ([#858](https://github.com/paradigmxyz/solar/issues/858))
- [sema] Cache override checker ([#856](https://github.com/paradigmxyz/solar/issues/856))
- Add diagnostic span helper ([#820](https://github.com/paradigmxyz/solar/issues/820))
- [sema] Preintern error type ([#811](https://github.com/paradigmxyz/solar/issues/811))
- [sema] Store empty variable docs
- [sema] Defer type flag checks ([#770](https://github.com/paradigmxyz/solar/issues/770))
- Replace index_vec with oxc_index ([#742](https://github.com/paradigmxyz/solar/issues/742))
- [sema] Split try_convert_explicit_to into two functions ([#719](https://github.com/paradigmxyz/solar/issues/719))
- [sema] Use one match in implicit conversion; use enum globs ([#631](https://github.com/paradigmxyz/solar/issues/631))
- [parser] Move mapping key type validation to semantic analysis ([#574](https://github.com/paradigmxyz/solar/issues/574))
- [interface] Update annotation_snippets emitter ([#602](https://github.com/paradigmxyz/solar/issues/602))
- [sema] Rename abi module to print ([#556](https://github.com/paradigmxyz/solar/issues/556))

### Styling

- [lsp] Add shared test fixtures ([#900](https://github.com/paradigmxyz/solar/issues/900))
- Revert "fix: address clippy and fmt CI failures"

### Testing

- [foundry] Use tempdirs for artifacts ([#902](https://github.com/paradigmxyz/solar/issues/902))
- Add FileCheck support to UI runner ([#890](https://github.com/paradigmxyz/solar/issues/890))
- Update ui test harness ([#845](https://github.com/paradigmxyz/solar/issues/845))
- Split ported UI tests ([#816](https://github.com/paradigmxyz/solar/issues/816))
- Normalize solc port annotations ([#779](https://github.com/paradigmxyz/solar/issues/779))
- Add implicit array conversions ([#714](https://github.com/paradigmxyz/solar/issues/714))
- Add unimplemented constructor test ([#692](https://github.com/paradigmxyz/solar/issues/692))
- Add bytes and contract type to mapping key type check test ([#636](https://github.com/paradigmxyz/solar/issues/636))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

Notable changes:
- Reduced size of AST for lower memory usage and better performance ([#500](https://github.com/paradigmxyz/solar/issues/500), [#546](https://github.com/paradigmxyz/solar/issues/546), [#497](https://github.com/paradigmxyz/solar/issues/497))
- Faster parsing, lexing ([#516](https://github.com/paradigmxyz/solar/issues/516), [#502](https://github.com/paradigmxyz/solar/issues/502), [#533](https://github.com/paradigmxyz/solar/issues/533))
- Implemented fmt::Debug for more types ([#537](https://github.com/paradigmxyz/solar/issues/537))

### Bug Fixes

- [sema] Remap Sources::file_to_id when sorting ([#548](https://github.com/paradigmxyz/solar/issues/548))
- [sema] Do not expose mutable Session access while entered ([#542](https://github.com/paradigmxyz/solar/issues/542))
- [diagnostics] Render footers at the bottom ([#538](https://github.com/paradigmxyz/solar/issues/538))
- [lexer] Str_from_to_end ([#532](https://github.com/paradigmxyz/solar/issues/532))
- [interface] Check longest context matches ([#529](https://github.com/paradigmxyz/solar/issues/529))
- [ast] Debug for Token ([#512](https://github.com/paradigmxyz/solar/issues/512))
- [CONTRIBUTING.md] Fmt check needs nightly toolchain ([#501](https://github.com/paradigmxyz/solar/issues/501))
- [ast] Store yul::Expr even if only Call is allowed ([#496](https://github.com/paradigmxyz/solar/issues/496))
- [sema] Peel parens when lowering call args ([#495](https://github.com/paradigmxyz/solar/issues/495))

### Dependencies

- [deps] Weekly `cargo update` ([#539](https://github.com/paradigmxyz/solar/issues/539))
- [deps] Weekly `cargo update` ([#534](https://github.com/paradigmxyz/solar/issues/534))
- Bump codspeed 4 ([#522](https://github.com/paradigmxyz/solar/issues/522))
- [deps] Weekly `cargo update` ([#511](https://github.com/paradigmxyz/solar/issues/511))

### Features

- [ast] Spanned optional commasep elements ([#543](https://github.com/paradigmxyz/solar/issues/543))
- [data-structures] Add ThinSlice ([#545](https://github.com/paradigmxyz/solar/issues/545))
- Add `ItemId::as_struct` ([#544](https://github.com/paradigmxyz/solar/issues/544))
- [sema] Allow delaying/manual import resolution ([#531](https://github.com/paradigmxyz/solar/issues/531))
- [sema] Add sources getters ([#528](https://github.com/paradigmxyz/solar/issues/528))
- Checks on upper bounds of contract storage sizes ([#169](https://github.com/paradigmxyz/solar/issues/169))
- [sema] Add validation for assembly memory-safe flags ([#263](https://github.com/paradigmxyz/solar/issues/263))
- Configurable logging destination ([#498](https://github.com/paradigmxyz/solar/issues/498))
- [parser] Introduce `Recovered` enum to improve code readability ([#517](https://github.com/paradigmxyz/solar/issues/517))

### Miscellaneous Tasks

- Add some traits to AstPath ([#549](https://github.com/paradigmxyz/solar/issues/549))
- Simplify pointer projection ([#541](https://github.com/paradigmxyz/solar/issues/541))
- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))
- [sema] Simplify perform_imports ([#530](https://github.com/paradigmxyz/solar/issues/530))
- [sema] Fix parse dbg log ([#527](https://github.com/paradigmxyz/solar/issues/527))
- Update benchmarks ([#523](https://github.com/paradigmxyz/solar/issues/523))
- [interface] Make SessionGlobals private ([#506](https://github.com/paradigmxyz/solar/issues/506))

### Other

- Use meta crate ([#526](https://github.com/paradigmxyz/solar/issues/526))
- Lowering benchmarks ([#521](https://github.com/paradigmxyz/solar/issues/521))
- Add source & parser capabilities ([#519](https://github.com/paradigmxyz/solar/issues/519))
- Update benchmarks ([#504](https://github.com/paradigmxyz/solar/issues/504))

### Performance

- [ast] Use ThinSlice ([#546](https://github.com/paradigmxyz/solar/issues/546))
- [sema] Pre-compute topo_sort map ([#533](https://github.com/paradigmxyz/solar/issues/533))
- [sema] Use a map in AST Sources ([#535](https://github.com/paradigmxyz/solar/issues/535))
- [parser] General improvements ([#516](https://github.com/paradigmxyz/solar/issues/516))
- [interface] Estimate capacity for SourceFile::lines ([#515](https://github.com/paradigmxyz/solar/issues/515))
- [parser] Pass Token in registers ([#509](https://github.com/paradigmxyz/solar/issues/509))
- [lexer] Avoid thread locals when we have a Session ([#507](https://github.com/paradigmxyz/solar/issues/507))
- [lexer] Use eat_until_either in eat_string ([#505](https://github.com/paradigmxyz/solar/issues/505))
- [lexer] Use lookup tables for char info ([#502](https://github.com/paradigmxyz/solar/issues/502))

### Refactor

- [ast] Boxed `yul::StmtKind::For` to reduce the size of `yul::Stmt` ([#500](https://github.com/paradigmxyz/solar/issues/500))

### Styling

- Implement fmt::Debug for more types ([#537](https://github.com/paradigmxyz/solar/issues/537))

### Testing

- Add unit test and debug assertion for sources inconsistency ([#550](https://github.com/paradigmxyz/solar/issues/550))
- Track node sizes ([#497](https://github.com/paradigmxyz/solar/issues/497))

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Bug Fixes

- [sema] Handle linearization failure fallout ([#472](https://github.com/paradigmxyz/solar/issues/472))
- Pin dependencies on ourselves

### Dependencies

- [deps] Weekly `cargo update` ([#482](https://github.com/paradigmxyz/solar/issues/482))
- [lexer] Inline token glueing into Cursor ([#479](https://github.com/paradigmxyz/solar/issues/479))
- [deps] Weekly `cargo update` ([#459](https://github.com/paradigmxyz/solar/issues/459))

### Features

- [interface] Add Session::reconfigure ([#491](https://github.com/paradigmxyz/solar/issues/491))
- [sema] Add Compiler::stage getter ([#490](https://github.com/paradigmxyz/solar/issues/490))
- Diagnostic suggestions ([#474](https://github.com/paradigmxyz/solar/issues/474))
- Add error format options for human-readable diagnostics ([#473](https://github.com/paradigmxyz/solar/issues/473))
- [interface] Impl Default for Session, create dcx from opts ([#471](https://github.com/paradigmxyz/solar/issues/471))
- [parser] Implement recursion depth limit for `expr`, `stmt`, and `yul_stmt` ([#464](https://github.com/paradigmxyz/solar/issues/464))
- Bump to annotate-snippets 0.12, diagnostic tweaks ([#465](https://github.com/paradigmxyz/solar/issues/465))
- [sema] Add `Compiler::enter{,_mut}_sequential` ([#457](https://github.com/paradigmxyz/solar/issues/457))
- Add another utility method for extracting diagnostics ([#455](https://github.com/paradigmxyz/solar/issues/455))
- [diagnostics] Track notes + expose notes/warn counts ([#447](https://github.com/paradigmxyz/solar/issues/447))
- `InMemoryEmitter` ([#451](https://github.com/paradigmxyz/solar/issues/451))

### Miscellaneous Tasks

- Improve 'parsed' debug log ([#489](https://github.com/paradigmxyz/solar/issues/489))
- Rename lexer methods to slop ([#481](https://github.com/paradigmxyz/solar/issues/481))
- [interface] Rename dcx flag setters ([#478](https://github.com/paradigmxyz/solar/issues/478))
- Update signal handler ([#463](https://github.com/paradigmxyz/solar/issues/463))
- Use json_type to sort ABIs ([#456](https://github.com/paradigmxyz/solar/issues/456))
- Move VERSION to config::version::SEMVER_VERSION and log it ([#454](https://github.com/paradigmxyz/solar/issues/454))
- Hide more source map implementation details ([#450](https://github.com/paradigmxyz/solar/issues/450))
- Chore!(data-structures): remove aliases in sync re-exports ([#452](https://github.com/paradigmxyz/solar/issues/452))
- Remove deprecated items ([#449](https://github.com/paradigmxyz/solar/issues/449))
- [meta] Update solidity links ([#448](https://github.com/paradigmxyz/solar/issues/448))
- Cap default threads at available_parallelism ([#445](https://github.com/paradigmxyz/solar/issues/445))
- Make all features reachable from meta crate ([#444](https://github.com/paradigmxyz/solar/issues/444))

### Performance

- Diagnostic suggestions ([#483](https://github.com/paradigmxyz/solar/issues/483))
- [lexer] Minor improvements ([#480](https://github.com/paradigmxyz/solar/issues/480))
- [interface] Cache thread pool inside of session ([#458](https://github.com/paradigmxyz/solar/issues/458))

### Testing

- Add CLI test suite ([#453](https://github.com/paradigmxyz/solar/issues/453))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

Notable changes:
- Rename enter to enter_sequential ([#392](https://github.com/paradigmxyz/solar/issues/392))
- Manual parser (`solar_parse`) usage is unchanged, but `solar_sema` API now goes through `Compiler`. See: Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))

### Bug Fixes

- Don't print fs errors twice ([#440](https://github.com/paradigmxyz/solar/issues/440))
- [ast] Visit array size ([#437](https://github.com/paradigmxyz/solar/issues/437))
- Error on dummy instead of empty args ([#407](https://github.com/paradigmxyz/solar/issues/407))
- OnDrop drops, rename to DropGuard ([#399](https://github.com/paradigmxyz/solar/issues/399))

### Dependencies

- [deps] Weekly `cargo update` ([#433](https://github.com/paradigmxyz/solar/issues/433))
- [deps] Weekly `cargo update` ([#402](https://github.com/paradigmxyz/solar/issues/402))
- [deps] Weekly `cargo update` ([#395](https://github.com/paradigmxyz/solar/issues/395))
- [deps] Weekly `cargo update` ([#393](https://github.com/paradigmxyz/solar/issues/393))
- [deps] Bump breakings ([#388](https://github.com/paradigmxyz/solar/issues/388))
- [deps] Weekly `cargo update` ([#387](https://github.com/paradigmxyz/solar/issues/387))
- [deps] Weekly `cargo update` ([#384](https://github.com/paradigmxyz/solar/issues/384))

### Documentation

- Typo in benchmarks ([#431](https://github.com/paradigmxyz/solar/issues/431))
- Link to benchmarks in readme ([#398](https://github.com/paradigmxyz/solar/issues/398))

### Features

- Add getters for source by file name (path) ([#442](https://github.com/paradigmxyz/solar/issues/442))
- [interface] Add FileLoader abstraction for fs/io ([#438](https://github.com/paradigmxyz/solar/issues/438))
- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Allow session mutable access ([#435](https://github.com/paradigmxyz/solar/issues/435))
- Make `Lit`erals implement `Copy` ([#414](https://github.com/paradigmxyz/solar/issues/414))
- Add ByteSymbol, use in LitKind::Str ([#425](https://github.com/paradigmxyz/solar/issues/425))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))
- Support `cargo binstall` ([#396](https://github.com/paradigmxyz/solar/issues/396))
- [sema] Add helper methods to Function ([#385](https://github.com/paradigmxyz/solar/issues/385))
- Visit_override when walking fn ([#383](https://github.com/paradigmxyz/solar/issues/383))

### Miscellaneous Tasks

- Update analyze_source_file ([#430](https://github.com/paradigmxyz/solar/issues/430))
- Downgrade some debug spans to trace ([#412](https://github.com/paradigmxyz/solar/issues/412))
- Abstract implementation of Declarations ([#410](https://github.com/paradigmxyz/solar/issues/410))
- Add some more `#[track_caller]` ([#409](https://github.com/paradigmxyz/solar/issues/409))
- Remove query tracing ([#406](https://github.com/paradigmxyz/solar/issues/406))
- Clean up contract inheritance linearization ([#405](https://github.com/paradigmxyz/solar/issues/405))
- Update docs, fix ci ([#403](https://github.com/paradigmxyz/solar/issues/403))
- Update cargo-dist and move off of fork ([#400](https://github.com/paradigmxyz/solar/issues/400))
- Rename enter to enter_sequential ([#392](https://github.com/paradigmxyz/solar/issues/392))
- Update benchmarks ([#390](https://github.com/paradigmxyz/solar/issues/390))

### Other

- Enforce typos ([#423](https://github.com/paradigmxyz/solar/issues/423))
- Update to `actions/checkout@v5` ([#404](https://github.com/paradigmxyz/solar/issues/404))

### Performance

- Load input source files in parallel ([#429](https://github.com/paradigmxyz/solar/issues/429))
- [sema] Better parallel parser scheduling ([#428](https://github.com/paradigmxyz/solar/issues/428))
- [parser] Improve parse_lit for integers ([#427](https://github.com/paradigmxyz/solar/issues/427))
- Tweak inlining ([#426](https://github.com/paradigmxyz/solar/issues/426))
- Pool symbol resolver scopes, refactor ([#413](https://github.com/paradigmxyz/solar/issues/413))
- [sema] Add some reserve calls ([#411](https://github.com/paradigmxyz/solar/issues/411))
- [sema] Use `Cell<usize>` in lowering ([#408](https://github.com/paradigmxyz/solar/issues/408))
- Implement likely/unlikely with `#[cold]` ([#386](https://github.com/paradigmxyz/solar/issues/386))

### Testing

- Add non existant import test ([#441](https://github.com/paradigmxyz/solar/issues/441))

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
