# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0](https://github.com/paradigmxyz/solar/releases/tag/v0.2.0)

### Bug Fixes

- [sema] Encode library function signatures like solc ([#926](https://github.com/paradigmxyz/solar/issues/926))
- [sema] Resolve public constant member over its getter ([#927](https://github.com/paradigmxyz/solar/issues/927))
- [parser] Recover malformed member access ([#914](https://github.com/paradigmxyz/solar/issues/914))
- [docs] Avoid rustdoc ICE on re-exports ([#912](https://github.com/paradigmxyz/solar/issues/912))
- [sema] Stabilize diagnostic order ([#879](https://github.com/paradigmxyz/solar/issues/879))
- Add codes to warnings ([#839](https://github.com/paradigmxyz/solar/issues/839))
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
- [sema] Canonicalize bare integer aliases ([#761](https://github.com/paradigmxyz/solar/issues/761))
- Address clippy and fmt CI failures
- [typeck] Explicit type check for payable address ([#690](https://github.com/paradigmxyz/solar/issues/690))
- [sema] Add handling for event/error failures ([#660](https://github.com/paradigmxyz/solar/issues/660))
- [typeck] Mark event/error typechecking todo ([#658](https://github.com/paradigmxyz/solar/issues/658))
- [lowering] Bring `this` and `super` into scope for constructor inheritance ([#570](https://github.com/paradigmxyz/solar/issues/570))

### Features

- [lsp] Add inlay hints ([#918](https://github.com/paradigmxyz/solar/issues/918))
- [lsp] Add auto-completion support ([#915](https://github.com/paradigmxyz/solar/issues/915))
- [lsp] Add symbol providers ([#906](https://github.com/paradigmxyz/solar/issues/906))
- [hir] Add pretty printer ([#903](https://github.com/paradigmxyz/solar/issues/903))
- Add HIR stats flag ([#893](https://github.com/paradigmxyz/solar/issues/893))
- Show AST enum variant sizes ([#892](https://github.com/paradigmxyz/solar/issues/892))
- [lsp] Add initial language server support ([#870](https://github.com/paradigmxyz/solar/issues/870))
- Add erc7201 builtin ([#877](https://github.com/paradigmxyz/solar/issues/877))
- Codegen ([#822](https://github.com/paradigmxyz/solar/issues/822))
- Continue after frontend errors ([#838](https://github.com/paradigmxyz/solar/issues/838))
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
- [typeck] Contracts implicit conversions ([#634](https://github.com/paradigmxyz/solar/issues/634))
- [typeck] Check lvalue ([#641](https://github.com/paradigmxyz/solar/issues/641))
- [typeck,ast_lowering] Constructor base arguments validation ([#580](https://github.com/paradigmxyz/solar/issues/580))
- Fix mapping ICE + impl data location coercion ([#637](https://github.com/paradigmxyz/solar/issues/637))
- [sema] Explicit bytes conversion ([#632](https://github.com/paradigmxyz/solar/issues/632))
- [sema] Address explicit conversion ([#626](https://github.com/paradigmxyz/solar/issues/626))
- [sema] Implement explicit conversions for fixed-size byte arrays ([#624](https://github.com/paradigmxyz/solar/issues/624))
- [sema] Enum explicit conversion ([#625](https://github.com/paradigmxyz/solar/issues/625))
- [sema] Implement array slice implicit conversion ([#623](https://github.com/paradigmxyz/solar/issues/623))
- [typeck] Payable address implicit conversion ([#622](https://github.com/paradigmxyz/solar/issues/622))
- Implement HIR builder ([#559](https://github.com/paradigmxyz/solar/issues/559))
- [ast] Naive natspec ([#470](https://github.com/paradigmxyz/solar/issues/470))
- [sema] Experimental typeck ([#563](https://github.com/paradigmxyz/solar/issues/563))
- Complete hir visitor ([#557](https://github.com/paradigmxyz/solar/issues/557))
- [ast] Add some methods to BinOpKind, DataLocation ([#555](https://github.com/paradigmxyz/solar/issues/555))

### Miscellaneous Tasks

- Dep upgrades ([#852](https://github.com/paradigmxyz/solar/issues/852))
- Display bits for `int_literal[n]` instead of bytes ([#672](https://github.com/paradigmxyz/solar/issues/672))
- [sema] Remove todo ([#656](https://github.com/paradigmxyz/solar/issues/656))
- [lowering] Allow empty sources ([#572](https://github.com/paradigmxyz/solar/issues/572))
- Fix -Zdump path parser ([#554](https://github.com/paradigmxyz/solar/issues/554))

### Other

- Revert "feat: SIMD optimizations, bug fixes, and struct codegen improvements"

### Performance

- [sema] Skip unnecessary visitor walks ([#772](https://github.com/paradigmxyz/solar/issues/772))

### Refactor

- Print stats with comfy-table ([#897](https://github.com/paradigmxyz/solar/issues/897))
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

- Revert "fix: address clippy and fmt CI failures"

### Testing

- Add implicit array conversions ([#714](https://github.com/paradigmxyz/solar/issues/714))

## [0.1.8](https://github.com/paradigmxyz/solar/releases/tag/v0.1.8)

### Bug Fixes

- [sema] Remap Sources::file_to_id when sorting ([#548](https://github.com/paradigmxyz/solar/issues/548))
- [sema] Do not expose mutable Session access while entered ([#542](https://github.com/paradigmxyz/solar/issues/542))
- [sema] Peel parens when lowering call args ([#495](https://github.com/paradigmxyz/solar/issues/495))

### Features

- [ast] Spanned optional commasep elements ([#543](https://github.com/paradigmxyz/solar/issues/543))
- Add `ItemId::as_struct` ([#544](https://github.com/paradigmxyz/solar/issues/544))
- [sema] Allow delaying/manual import resolution ([#531](https://github.com/paradigmxyz/solar/issues/531))
- [sema] Add sources getters ([#528](https://github.com/paradigmxyz/solar/issues/528))
- Checks on upper bounds of contract storage sizes ([#169](https://github.com/paradigmxyz/solar/issues/169))
- [sema] Add validation for assembly memory-safe flags ([#263](https://github.com/paradigmxyz/solar/issues/263))

### Miscellaneous Tasks

- Add some traits to AstPath ([#549](https://github.com/paradigmxyz/solar/issues/549))
- Simplify pointer projection ([#541](https://github.com/paradigmxyz/solar/issues/541))
- Remove feature(doc_auto_cfg) ([#540](https://github.com/paradigmxyz/solar/issues/540))
- [sema] Simplify perform_imports ([#530](https://github.com/paradigmxyz/solar/issues/530))
- [sema] Fix parse dbg log ([#527](https://github.com/paradigmxyz/solar/issues/527))

### Performance

- [ast] Use ThinSlice ([#546](https://github.com/paradigmxyz/solar/issues/546))
- [sema] Pre-compute topo_sort map ([#533](https://github.com/paradigmxyz/solar/issues/533))
- [sema] Use a map in AST Sources ([#535](https://github.com/paradigmxyz/solar/issues/535))

### Styling

- Implement fmt::Debug for more types ([#537](https://github.com/paradigmxyz/solar/issues/537))

### Testing

- Add unit test and debug assertion for sources inconsistency ([#550](https://github.com/paradigmxyz/solar/issues/550))
- Track node sizes ([#497](https://github.com/paradigmxyz/solar/issues/497))

## [0.1.7](https://github.com/paradigmxyz/solar/releases/tag/v0.1.7)

### Bug Fixes

- [sema] Handle linearization failure fallout ([#472](https://github.com/paradigmxyz/solar/issues/472))

### Features

- [sema] Add Compiler::stage getter ([#490](https://github.com/paradigmxyz/solar/issues/490))
- Diagnostic suggestions ([#474](https://github.com/paradigmxyz/solar/issues/474))
- Bump to annotate-snippets 0.12, diagnostic tweaks ([#465](https://github.com/paradigmxyz/solar/issues/465))
- [sema] Add `Compiler::enter{,_mut}_sequential` ([#457](https://github.com/paradigmxyz/solar/issues/457))

### Miscellaneous Tasks

- Improve 'parsed' debug log ([#489](https://github.com/paradigmxyz/solar/issues/489))
- Use json_type to sort ABIs ([#456](https://github.com/paradigmxyz/solar/issues/456))
- Move VERSION to config::version::SEMVER_VERSION and log it ([#454](https://github.com/paradigmxyz/solar/issues/454))
- Hide more source map implementation details ([#450](https://github.com/paradigmxyz/solar/issues/450))
- Chore!(data-structures): remove aliases in sync re-exports ([#452](https://github.com/paradigmxyz/solar/issues/452))
- [meta] Update solidity links ([#448](https://github.com/paradigmxyz/solar/issues/448))
- Make all features reachable from meta crate ([#444](https://github.com/paradigmxyz/solar/issues/444))

### Performance

- [interface] Cache thread pool inside of session ([#458](https://github.com/paradigmxyz/solar/issues/458))

## [0.1.6](https://github.com/paradigmxyz/solar/releases/tag/v0.1.6)

### Bug Fixes

- Error on dummy instead of empty args ([#407](https://github.com/paradigmxyz/solar/issues/407))
- OnDrop drops, rename to DropGuard ([#399](https://github.com/paradigmxyz/solar/issues/399))

### Features

- Add getters for source by file name (path) ([#442](https://github.com/paradigmxyz/solar/issues/442))
- [interface] Add FileLoader abstraction for fs/io ([#438](https://github.com/paradigmxyz/solar/issues/438))
- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Allow session mutable access ([#435](https://github.com/paradigmxyz/solar/issues/435))
- Make `Lit`erals implement `Copy` ([#414](https://github.com/paradigmxyz/solar/issues/414))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))
- [sema] Add helper methods to Function ([#385](https://github.com/paradigmxyz/solar/issues/385))

### Miscellaneous Tasks

- Downgrade some debug spans to trace ([#412](https://github.com/paradigmxyz/solar/issues/412))
- Abstract implementation of Declarations ([#410](https://github.com/paradigmxyz/solar/issues/410))
- Remove query tracing ([#406](https://github.com/paradigmxyz/solar/issues/406))
- Clean up contract inheritance linearization ([#405](https://github.com/paradigmxyz/solar/issues/405))

### Other

- Enforce typos ([#423](https://github.com/paradigmxyz/solar/issues/423))

### Performance

- Load input source files in parallel ([#429](https://github.com/paradigmxyz/solar/issues/429))
- [sema] Better parallel parser scheduling ([#428](https://github.com/paradigmxyz/solar/issues/428))
- Pool symbol resolver scopes, refactor ([#413](https://github.com/paradigmxyz/solar/issues/413))
- [sema] Add some reserve calls ([#411](https://github.com/paradigmxyz/solar/issues/411))
- [sema] Use `Cell<usize>` in lowering ([#408](https://github.com/paradigmxyz/solar/issues/408))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Bug Fixes

- Visit modifiers in hir visitor ([#373](https://github.com/paradigmxyz/solar/issues/373))

### Dependencies

- Bump to edition 2024, MSRV 1.88 ([#375](https://github.com/paradigmxyz/solar/issues/375))
- [deps] Weekly `cargo update` ([#351](https://github.com/paradigmxyz/solar/issues/351))

### Features

- Resolve ctor base args ([#322](https://github.com/paradigmxyz/solar/issues/322))
- Add span visitor debug tool ([#355](https://github.com/paradigmxyz/solar/issues/355))

### Miscellaneous Tasks

- Use Option<StateMutability> in the AST ([#374](https://github.com/paradigmxyz/solar/issues/374))
- Fn header spans ([#371](https://github.com/paradigmxyz/solar/issues/371))
- Misc cleanup, util methods ([#367](https://github.com/paradigmxyz/solar/issues/367))
- Add span to `TryCatchClause` ([#364](https://github.com/paradigmxyz/solar/issues/364))
- [parser] Move unescaping from lexer to parser ([#360](https://github.com/paradigmxyz/solar/issues/360))

## [0.1.4](https://github.com/paradigmxyz/solar/releases/tag/v0.1.4)

### Bug Fixes

- [sema] Don't warn 3628 if no interface functions ([#330](https://github.com/paradigmxyz/solar/issues/330))

### Features

- [sema] Implement receive function checks ([#321](https://github.com/paradigmxyz/solar/issues/321))
- [sema] Display more types, add Ty::display ([#328](https://github.com/paradigmxyz/solar/issues/328))
- Add span in FunctionHeader ([#318](https://github.com/paradigmxyz/solar/issues/318))
- [ast] Add spans to blocks ([#314](https://github.com/paradigmxyz/solar/issues/314))
- Typecheck for external type clashes ([#312](https://github.com/paradigmxyz/solar/issues/312))

## [0.1.3](https://github.com/paradigmxyz/solar/releases/tag/v0.1.3)

### Bug Fixes

- [sema] Declare selfdestruct builtin ([#297](https://github.com/paradigmxyz/solar/issues/297))
- [sema] Correct variable declaration scopes ([#300](https://github.com/paradigmxyz/solar/issues/300))
- Invalid underscores check ([#281](https://github.com/paradigmxyz/solar/issues/281))

### Features

- Warn when payable fallback function found but no receive function ([#170](https://github.com/paradigmxyz/solar/issues/170))
- Literal checks ([#287](https://github.com/paradigmxyz/solar/issues/287))
- [ast] Store concatenated string literals ([#266](https://github.com/paradigmxyz/solar/issues/266))
- [ast] Add span to CallArgs ([#265](https://github.com/paradigmxyz/solar/issues/265))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))

### Miscellaneous Tasks

- Shorter Debug impls ([#291](https://github.com/paradigmxyz/solar/issues/291))
- Misc map cleanups ([#277](https://github.com/paradigmxyz/solar/issues/277))
- Add some more non_exhaustive ([#261](https://github.com/paradigmxyz/solar/issues/261))
- Tweak tracing events ([#255](https://github.com/paradigmxyz/solar/issues/255))
- Revert extra changelog

## [0.1.2](https://github.com/paradigmxyz/solar/releases/tag/v0.1.2)

### Bug Fixes

- Hir call visiting ([#250](https://github.com/paradigmxyz/solar/issues/250))
- Public transient assert ([#248](https://github.com/paradigmxyz/solar/issues/248))
- [parser] Glob imports require an alias ([#245](https://github.com/paradigmxyz/solar/issues/245))
- Point resolution errors to the import string ([#244](https://github.com/paradigmxyz/solar/issues/244))
- Unsound transmute ([#239](https://github.com/paradigmxyz/solar/issues/239))
- Variable getter functions are external ([#202](https://github.com/paradigmxyz/solar/issues/202))
- Error when func type has named return param  ([#173](https://github.com/paradigmxyz/solar/issues/173))
- Don't check placeholders in virtual modifiers ([#201](https://github.com/paradigmxyz/solar/issues/201))
- Correctly resolve try/catch scopes ([#200](https://github.com/paradigmxyz/solar/issues/200))
- Correctly evaluate public constants ([#187](https://github.com/paradigmxyz/solar/issues/187))
- Allow storage in modifiers ([#185](https://github.com/paradigmxyz/solar/issues/185))

### Dependencies

- Bump solidity submodule to 0.8.29 ([#230](https://github.com/paradigmxyz/solar/issues/230))
- [deps] Weekly `cargo update` ([#225](https://github.com/paradigmxyz/solar/issues/225))

### Features

- Align input options with solc, implement remapping context ([#238](https://github.com/paradigmxyz/solar/issues/238))
- Refactor FileResolver to allow custom current_dir ([#235](https://github.com/paradigmxyz/solar/issues/235))
- [hir] Expose hir from parsing context ([#210](https://github.com/paradigmxyz/solar/issues/210))
- Parse storage layout specifiers ([#232](https://github.com/paradigmxyz/solar/issues/232))
- Make current_dir configurable ([#231](https://github.com/paradigmxyz/solar/issues/231))
- Implement minimal HIR visitor ([#195](https://github.com/paradigmxyz/solar/issues/195))

## [0.1.1](https://github.com/paradigmxyz/solar/releases/tag/v0.1.1)

### Bug Fixes

- Display order in AST stats ([#180](https://github.com/paradigmxyz/solar/issues/180))
- Reduce width of subnode name in ast-stats ([#157](https://github.com/paradigmxyz/solar/issues/157))
- Exclude arrays from mapping getter returns ([#148](https://github.com/paradigmxyz/solar/issues/148))
- Properly handle recursive types ([#133](https://github.com/paradigmxyz/solar/issues/133))
- Validate placeholder is within modifier ([#132](https://github.com/paradigmxyz/solar/issues/132))

### Documentation

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
- Update to Rust 1.83 ([#150](https://github.com/paradigmxyz/solar/issues/150))
- Validate variable data locations ([#149](https://github.com/paradigmxyz/solar/issues/149))
- Make TyAbiPrinter public ([#145](https://github.com/paradigmxyz/solar/issues/145))
- Add some methods to CallArgs ([#140](https://github.com/paradigmxyz/solar/issues/140))
- Print AST statistics with -Zast-stats flag ([#125](https://github.com/paradigmxyz/solar/issues/125))
- Return ControlFlow from AST visitor methods ([#115](https://github.com/paradigmxyz/solar/issues/115))

### Miscellaneous Tasks

- Add TyAbiPrinterMode ([#147](https://github.com/paradigmxyz/solar/issues/147))
- Mark TyKind as non_exhaustive ([#146](https://github.com/paradigmxyz/solar/issues/146))
- Use unimplemented! instead of todo! in eval.rs ([#110](https://github.com/paradigmxyz/solar/issues/110))

### Other

- Initial AST validation for using-for ([#119](https://github.com/paradigmxyz/solar/issues/119))
- AST validate that a contract does not have a function with contract name ([#117](https://github.com/paradigmxyz/solar/issues/117))
- Validate num. variants in enum declaration ([#120](https://github.com/paradigmxyz/solar/issues/120))
- Better error for struct without any fields ([#121](https://github.com/paradigmxyz/solar/issues/121))
- AST validate unchecked nested blocks ([#116](https://github.com/paradigmxyz/solar/issues/116))
- AST validate check statement only in while/for loops ([#111](https://github.com/paradigmxyz/solar/issues/111))

### Refactor

- Split Ty printers ([#144](https://github.com/paradigmxyz/solar/issues/144))
- Re-export ast::* internal module ([#141](https://github.com/paradigmxyz/solar/issues/141))

### Testing

- Add some more tests ([#122](https://github.com/paradigmxyz/solar/issues/122))

## [0.1.0](https://github.com/paradigmxyz/solar/releases/tag/v0.1.0)

Initial release.

<!-- generated by git-cliff -->
