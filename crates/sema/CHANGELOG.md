# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

### Bug Fixes

- Honor --stop-after for lowering and analysis ([#2243](https://github.com/paradigmxyz/solar/issues/2243))
- [codegen] Protect spills from writes the compiler cannot place ([#2006](https://github.com/paradigmxyz/solar/issues/2006))
- Emit diagnostics in the same order at every thread count ([#2131](https://github.com/paradigmxyz/solar/issues/2131))
- [sema] Reject every unfolded constant in asm ([#1900](https://github.com/paradigmxyz/solar/issues/1900))
- [sema] Reject out-of-range literal indexes ([#1898](https://github.com/paradigmxyz/solar/issues/1898))
- [sema] Drop cached output allocations again ([#1889](https://github.com/paradigmxyz/solar/issues/1889))
- [sema] Skip version check on bad pragmas ([#1869](https://github.com/paradigmxyz/solar/issues/1869))
- [sema] Keep UDVT names in mapping signatures ([#1856](https://github.com/paradigmxyz/solar/issues/1856))
- [sema] Stop ABI recursion on recursive structs ([#1853](https://github.com/paradigmxyz/solar/issues/1853))
- [interface] Resolve imports like solc ([#1740](https://github.com/paradigmxyz/solar/issues/1740))
- [sema] Report Yul reads of Solidity locals ([#1798](https://github.com/paradigmxyz/solar/issues/1798))
- [sema] Scope function type parameter names ([#1835](https://github.com/paradigmxyz/solar/issues/1835))
- [sema] Report direct struct cycles next to arrays ([#1819](https://github.com/paradigmxyz/solar/issues/1819))
- [sema] Size memory types as solc does ([#1830](https://github.com/paradigmxyz/solar/issues/1830))
- [sema] Reject payable internal function types ([#1803](https://github.com/paradigmxyz/solar/issues/1803))
- [sema] Reject literals without a mobile type ([#1832](https://github.com/paradigmxyz/solar/issues/1832))
- [sema] Type-check storage layout base slots ([#1775](https://github.com/paradigmxyz/solar/issues/1775))
- [sema] Stop folding bytesN constants ([#1784](https://github.com/paradigmxyz/solar/issues/1784))
- [sema] Report reserved names like solc ([#1768](https://github.com/paradigmxyz/solar/issues/1768))
- [sema] Reject delete on storage pointers ([#1822](https://github.com/paradigmxyz/solar/issues/1822))
- [sema] Hide getters behind foreign contract names ([#1777](https://github.com/paradigmxyz/solar/issues/1777))
- [sema] Skip function types in struct recursion ([#1790](https://github.com/paradigmxyz/solar/issues/1790))
- [sema] Add solc codes to Yul reference checks ([#1793](https://github.com/paradigmxyz/solar/issues/1793))
- [sema] Reject invalid function type visibility ([#1795](https://github.com/paradigmxyz/solar/issues/1795))
- [sema] Add solc codes to operator errors ([#1797](https://github.com/paradigmxyz/solar/issues/1797))
- [sema] Report duplicate enum values ([#1801](https://github.com/paradigmxyz/solar/issues/1801))
- [sema] Skip Yul reference checks on errors ([#1806](https://github.com/paradigmxyz/solar/issues/1806))
- [sema] Reject events that share a function name ([#1807](https://github.com/paradigmxyz/solar/issues/1807))
- [sema] Give declaration conflicts solc's codes ([#1812](https://github.com/paradigmxyz/solar/issues/1812))
- [sema] Reject delete on mappings ([#1816](https://github.com/paradigmxyz/solar/issues/1816))
- [sema] Reject named arguments in conversions ([#1828](https://github.com/paradigmxyz/solar/issues/1828))
- [sema] Reject inheriting a custom storage layout ([#1818](https://github.com/paradigmxyz/solar/issues/1818))
- [sema] Reject unencodable function values ([#1820](https://github.com/paradigmxyz/solar/issues/1820))
- [sema] Reject struct comparisons ([#1787](https://github.com/paradigmxyz/solar/issues/1787))
- [sema] Reject leave outside Yul functions ([#1788](https://github.com/paradigmxyz/solar/issues/1788))
- [sema] Reject duplicate Yul declarations ([#1789](https://github.com/paradigmxyz/solar/issues/1789))
- Accept paren errors and module ternaries ([#1779](https://github.com/paradigmxyz/solar/issues/1779))
- [sema] Check special function signatures ([#1767](https://github.com/paradigmxyz/solar/issues/1767))
- [sema] Reject library names as types and bases ([#1772](https://github.com/paradigmxyz/solar/issues/1772))
- [sema] Stop error cascades from bad operands ([#1764](https://github.com/paradigmxyz/solar/issues/1764))
- [sema] Skip explicit casts of error operands ([#1762](https://github.com/paradigmxyz/solar/issues/1762))
- [sema] Reject Yul break outside for loops ([#1747](https://github.com/paradigmxyz/solar/issues/1747))
- [sema] Check function type parameter locations ([#1711](https://github.com/paradigmxyz/solar/issues/1711))
- [parse] Parse `fixedMxN` and `ufixedMxN` types ([#1724](https://github.com/paradigmxyz/solar/issues/1724))
- [sema] Ignore underscores in address checksums ([#1715](https://github.com/paradigmxyz/solar/issues/1715))
- [sema] Type `~` on integer literals by value ([#1722](https://github.com/paradigmxyz/solar/issues/1722))
- [sema] Allow underscores around uppercase `E` ([#1718](https://github.com/paradigmxyz/solar/issues/1718))
- [sema] Declare module aliases before imports ([#1717](https://github.com/paradigmxyz/solar/issues/1717))
- [sema] Resolve qualified emits to own members ([#1714](https://github.com/paradigmxyz/solar/issues/1714))
- [sema] Resolve Yul calls only to Yul names ([#1710](https://github.com/paradigmxyz/solar/issues/1710))
- [sema] Keep string literal length in its type ([#1707](https://github.com/paradigmxyz/solar/issues/1707))
- [codegen] Use full-width storage array layouts ([#1687](https://github.com/paradigmxyz/solar/issues/1687))
- [sema] Preserve library ABI type names ([#1590](https://github.com/paradigmxyz/solar/issues/1590))
- [parser] Keep Yul builtin name checks ([#1584](https://github.com/paradigmxyz/solar/issues/1584))
- [sema] Give tuple assignments the void type ([#1572](https://github.com/paradigmxyz/solar/issues/1572))
- Validate Yul builtin names in sema ([#1568](https://github.com/paradigmxyz/solar/issues/1568))
- [sema] Convert empty strings to fixed bytes ([#1573](https://github.com/paradigmxyz/solar/issues/1573))
- [sema] Exclude external-only unqualified names ([#1577](https://github.com/paradigmxyz/solar/issues/1577))
- [sema] Resolve named constructor arguments ([#1571](https://github.com/paradigmxyz/solar/issues/1571))
- [sema] Limit qualified contract functions ([#1578](https://github.com/paradigmxyz/solar/issues/1578))
- [sema] Exclude Yul functions from overrides ([#1570](https://github.com/paradigmxyz/solar/issues/1570))
- [sema] Lower boolean Yul conditions ([#1569](https://github.com/paradigmxyz/solar/issues/1569))
- [sema] Handle abstract IDs and tuple scope ([#1537](https://github.com/paradigmxyz/solar/issues/1537))
- Retain recursive library storage methods ([#1534](https://github.com/paradigmxyz/solar/issues/1534))
- Respect inherited function visibility ([#1531](https://github.com/paradigmxyz/solar/issues/1531))
- Canonicalize library event and error hashes ([#1532](https://github.com/paradigmxyz/solar/issues/1532))
- Evaluate zero to zero as one ([#1533](https://github.com/paradigmxyz/solar/issues/1533))
- [lsp] Complete symbols in incomplete code ([#1475](https://github.com/paradigmxyz/solar/issues/1475))
- [lsp] Index NatSpec symbol references ([#1486](https://github.com/paradigmxyz/solar/issues/1486))
- Address the review findings of the audit split ([#1383](https://github.com/paradigmxyz/solar/issues/1383))
- [sema] Accept programs solc accepts in the type checker ([#1365](https://github.com/paradigmxyz/solar/issues/1365))
- [sema] Check the try returns clause against the callee ([#1372](https://github.com/paradigmxyz/solar/issues/1372))
- [codegen] Return library storage pointers as slots ([#1369](https://github.com/paradigmxyz/solar/issues/1369))
- [codegen] Match solc's external calls before byzantium ([#1367](https://github.com/paradigmxyz/solar/issues/1367))
- Check and lower base constructor arguments like solc ([#1364](https://github.com/paradigmxyz/solar/issues/1364))
- [sema] Add solc's interface and free function checks ([#1370](https://github.com/paradigmxyz/solar/issues/1370))
- [sema] Match solc library ABI and declaration checks ([#1363](https://github.com/paradigmxyz/solar/issues/1363))
- [sema] Drop cached output allocations ([#1349](https://github.com/paradigmxyz/solar/issues/1349))
- [sema] Defer ternary branch coercion ([#1338](https://github.com/paradigmxyz/solar/issues/1338))
- [codegen] Legalize legacy EVM shifts ([#1267](https://github.com/paradigmxyz/solar/issues/1267))
- [sema] Reject dynamic calldata slices ([#1303](https://github.com/paradigmxyz/solar/issues/1303))
- Revert accidental Yul EVM version merge
- [yul] Complete EVM version checks
- [sema] Downgrade NatSpec errors to warnings ([#1281](https://github.com/paradigmxyz/solar/issues/1281))
- [sema] Resolve overridden call targets ([#1264](https://github.com/paradigmxyz/solar/issues/1264))
- [sema] Complete EVM version checks ([#1278](https://github.com/paradigmxyz/solar/issues/1278))
- [sema] Gate high-level EVM features ([#1275](https://github.com/paradigmxyz/solar/issues/1275))
- [sema] Match solc fallback and receive overrides ([#1265](https://github.com/paradigmxyz/solar/issues/1265))
- [sema] Allow calldata string slices ([#1256](https://github.com/paradigmxyz/solar/issues/1256))
- [yul] Honor Cancun builtin versions ([#1252](https://github.com/paradigmxyz/solar/issues/1252))
- [sema] Include referenced ABI items ([#1103](https://github.com/paradigmxyz/solar/issues/1103))
- [codegen] Remove lowering-time inlining ([#1082](https://github.com/paradigmxyz/solar/issues/1082))
- [sema] Align project edge cases with solc ([#1088](https://github.com/paradigmxyz/solar/issues/1088))
- [ast] Reject unchecked as bare loop/if body ([#1064](https://github.com/paradigmxyz/solar/issues/1064))
- [sema] Include library mapping params in hashes ([#1061](https://github.com/paradigmxyz/solar/issues/1061))
- [sema] Reject internal types in external fn types ([#1063](https://github.com/paradigmxyz/solar/issues/1063))
- [lsp] Add missing symbol hovers ([#1054](https://github.com/paradigmxyz/solar/issues/1054))
- [typeck] Distinguish storage copies ([#974](https://github.com/paradigmxyz/solar/issues/974))
- [typeck] Validate return conversions ([#975](https://github.com/paradigmxyz/solar/issues/975))
- [typeck] Reject non-mobile functions ([#976](https://github.com/paradigmxyz/solar/issues/976))
- [typeck] Tighten explicit conversions ([#977](https://github.com/paradigmxyz/solar/issues/977))
- [sema] Match solc storage layouts ([#945](https://github.com/paradigmxyz/solar/issues/945))
- [typeck] Type Yul bool literals as words ([#937](https://github.com/paradigmxyz/solar/issues/937))
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

### Dependencies

- [deps] Update num-bigint and comfy-table ([#1261](https://github.com/paradigmxyz/solar/issues/1261))

### Features

- Add the solar NatSpec tags ([#2101](https://github.com/paradigmxyz/solar/issues/2101))
- [sema] Evaluate constants after type checking ([#1896](https://github.com/paradigmxyz/solar/issues/1896))
- [sema] Rewrite constant evaluation ([#1878](https://github.com/paradigmxyz/solar/issues/1878))
- Support fractional number literals ([#1752](https://github.com/paradigmxyz/solar/issues/1752))
- [sema] Warn on unmatched version pragmas ([#1761](https://github.com/paradigmxyz/solar/issues/1761))
- [sema] Bound ABI head sizes ([#1774](https://github.com/paradigmxyz/solar/issues/1774))
- [sema] Reject index access on library types ([#1799](https://github.com/paradigmxyz/solar/issues/1799))
- [sema] Warn on msg.gas instead of rejecting it ([#1837](https://github.com/paradigmxyz/solar/issues/1837))
- [sema] Reject msg.gas with error 1400 ([#1778](https://github.com/paradigmxyz/solar/issues/1778))
- [sema] Report mapping parameter name conflicts ([#1794](https://github.com/paradigmxyz/solar/issues/1794))
- [sema] Reject modifiers used as values ([#1796](https://github.com/paradigmxyz/solar/issues/1796))
- [sema] Reject static calls of bodiless modifiers ([#1802](https://github.com/paradigmxyz/solar/issues/1802))
- [sema] Add solc codes to overload errors ([#1810](https://github.com/paradigmxyz/solar/issues/1810))
- [sema] Add solc's code to abi.decode type error ([#1821](https://github.com/paradigmxyz/solar/issues/1821))
- [sema] Add solc's code to type size errors ([#1823](https://github.com/paradigmxyz/solar/issues/1823))
- [sema] Warn that ABI coder v1 is not supported ([#1785](https://github.com/paradigmxyz/solar/issues/1785))
- [sema] Check event and error declarations ([#1766](https://github.com/paradigmxyz/solar/issues/1766))
- [sema] Check constant initializers like solc ([#1771](https://github.com/paradigmxyz/solar/issues/1771))
- [sema] Port solc's control flow analyzer ([#1737](https://github.com/paradigmxyz/solar/issues/1737))
- [lsp] Add builtin hover ([#1646](https://github.com/paradigmxyz/solar/issues/1646))
- Expose rooted struct storage layouts ([#1413](https://github.com/paradigmxyz/solar/issues/1413))
- [cli] Add bytecode metadata ([#1344](https://github.com/paradigmxyz/solar/issues/1344))
- [codegen] Add shared program data ([#1304](https://github.com/paradigmxyz/solar/issues/1304))
- [codegen] Implement Amsterdam EVM version ([#1288](https://github.com/paradigmxyz/solar/issues/1288))
- [test] Add external foundry testing infra ([#1213](https://github.com/paradigmxyz/solar/issues/1213))
- Enable more codegen benches ([#1092](https://github.com/paradigmxyz/solar/issues/1092))
- [cli] Merge IR optimization commands ([#1080](https://github.com/paradigmxyz/solar/issues/1080))
- [lsp] Add call hierarchy support ([#1035](https://github.com/paradigmxyz/solar/issues/1035))
- [codegen] Support storage array push ([#1033](https://github.com/paradigmxyz/solar/issues/1033))
-  feat(lsp): add hover support ([#987](https://github.com/paradigmxyz/solar/issues/987))
- View/pure checker ([#980](https://github.com/paradigmxyz/solar/issues/980))
- [sema] Add call argument query ([#955](https://github.com/paradigmxyz/solar/issues/955))
- [sema] Add lint query helpers ([#954](https://github.com/paradigmxyz/solar/issues/954))
- [lsp] Add semantic rename support ([#950](https://github.com/paradigmxyz/solar/issues/950))
- [sema] Add literal eval query ([#952](https://github.com/paradigmxyz/solar/issues/952))
- [sema] Add const value accessors ([#953](https://github.com/paradigmxyz/solar/issues/953))
- [sema] Enable type checking by default ([#939](https://github.com/paradigmxyz/solar/issues/939))
- [cli] Emit docs and storage layout ([#933](https://github.com/paradigmxyz/solar/issues/933))
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

- Fix clippy unnecessary_lazy_evaluations ([#2227](https://github.com/paradigmxyz/solar/issues/2227))
- Drop solc mentions from obvious comments ([#1864](https://github.com/paradigmxyz/solar/issues/1864))
- Fix merge conflicts
- Release 0.2.0
- Dep upgrades ([#852](https://github.com/paradigmxyz/solar/issues/852))
- Display bits for `int_literal[n]` instead of bytes ([#672](https://github.com/paradigmxyz/solar/issues/672))
- [sema] Remove todo ([#656](https://github.com/paradigmxyz/solar/issues/656))
- [lowering] Allow empty sources ([#572](https://github.com/paradigmxyz/solar/issues/572))
- Fix -Zdump path parser ([#554](https://github.com/paradigmxyz/solar/issues/554))

### Other

- Revert "fix: revert accidental Yul EVM version merge" ([#1287](https://github.com/paradigmxyz/solar/issues/1287))
- Revert "feat: SIMD optimizations, bug fixes, and struct codegen improvements"

### Performance

- [sema] Cache attached functions per type and scope ([#2109](https://github.com/paradigmxyz/solar/issues/2109))
- [sema] Parallelize type checking at item grain ([#2061](https://github.com/paradigmxyz/solar/issues/2061))
- [codegen] Share stack-reschedule searches across programs ([#2009](https://github.com/paradigmxyz/solar/issues/2009))
- [sema] Pick query caches by key type ([#1857](https://github.com/paradigmxyz/solar/issues/1857))
- [codegen] Speed up MIR and EVM IR passes ([#1556](https://github.com/paradigmxyz/solar/issues/1556))
- [lsp] Reduce repeated analysis work ([#1461](https://github.com/paradigmxyz/solar/issues/1461))
- [lsp] Reduce query work and index memory ([#1395](https://github.com/paradigmxyz/solar/issues/1395))
- [codegen] Use dense sets for IDs ([#958](https://github.com/paradigmxyz/solar/issues/958))
- [sema] Skip unnecessary visitor walks ([#772](https://github.com/paradigmxyz/solar/issues/772))

### Refactor

- [mir] One load and store model for memory ([#1755](https://github.com/paradigmxyz/solar/issues/1755))
- [sema] Check mapping names in AST validator ([#1839](https://github.com/paradigmxyz/solar/issues/1839))
- [sema] Check function type visibility in AST validator ([#1838](https://github.com/paradigmxyz/solar/issues/1838))
- Restructure tracing spans ([#1633](https://github.com/paradigmxyz/solar/issues/1633))
- [data-structures] Re-export std::fmt::from_fn ([#1617](https://github.com/paradigmxyz/solar/issues/1617))
- [hir] Represent call options once ([#1580](https://github.com/paradigmxyz/solar/issues/1580))
- [codegen] Rewrite MIR codegen ([#1161](https://github.com/paradigmxyz/solar/issues/1161))
- Centralize function naming ([#1341](https://github.com/paradigmxyz/solar/issues/1341))
- [codegen] Stop creating dummy values ([#1036](https://github.com/paradigmxyz/solar/issues/1036))
- [lsp] Share HIR printer for hovers ([#1056](https://github.com/paradigmxyz/solar/issues/1056))
- [codegen] Streamline EVM IR pipeline ([#990](https://github.com/paradigmxyz/solar/issues/990))
- Use mod.rs for nested modules ([#979](https://github.com/paradigmxyz/solar/issues/979))
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

- [sema] Cover mixed inline array elements ([#1868](https://github.com/paradigmxyz/solar/issues/1868))
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

- Release 0.1.8 ([#551](https://github.com/paradigmxyz/solar/issues/551))
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
- Error on dummy instead of empty args ([#407](https://github.com/paradigmxyz/solar/issues/407))
- OnDrop drops, rename to DropGuard ([#399](https://github.com/paradigmxyz/solar/issues/399))

### Features

- [ast] Add span to yul switch cases ([#493](https://github.com/paradigmxyz/solar/issues/493))
- [sema] Add Compiler::stage getter ([#490](https://github.com/paradigmxyz/solar/issues/490))
- Diagnostic suggestions ([#474](https://github.com/paradigmxyz/solar/issues/474))
- Bump to annotate-snippets 0.12, diagnostic tweaks ([#465](https://github.com/paradigmxyz/solar/issues/465))
- [sema] Add `Compiler::enter{,_mut}_sequential` ([#457](https://github.com/paradigmxyz/solar/issues/457))
- Add getters for source by file name (path) ([#442](https://github.com/paradigmxyz/solar/issues/442))
- [interface] Add FileLoader abstraction for fs/io ([#438](https://github.com/paradigmxyz/solar/issues/438))
- Implement base_path, streamline creating pcx ([#436](https://github.com/paradigmxyz/solar/issues/436))
- Allow session mutable access ([#435](https://github.com/paradigmxyz/solar/issues/435))
- Make `Lit`erals implement `Copy` ([#414](https://github.com/paradigmxyz/solar/issues/414))
- Add Compiler ([#397](https://github.com/paradigmxyz/solar/issues/397))
- [sema] Add helper methods to Function ([#385](https://github.com/paradigmxyz/solar/issues/385))

### Miscellaneous Tasks

- Release 0.1.7 ([#492](https://github.com/paradigmxyz/solar/issues/492))
- Improve 'parsed' debug log ([#489](https://github.com/paradigmxyz/solar/issues/489))
- Use json_type to sort ABIs ([#456](https://github.com/paradigmxyz/solar/issues/456))
- Move VERSION to config::version::SEMVER_VERSION and log it ([#454](https://github.com/paradigmxyz/solar/issues/454))
- Hide more source map implementation details ([#450](https://github.com/paradigmxyz/solar/issues/450))
- Chore!(data-structures): remove aliases in sync re-exports ([#452](https://github.com/paradigmxyz/solar/issues/452))
- [meta] Update solidity links ([#448](https://github.com/paradigmxyz/solar/issues/448))
- Make all features reachable from meta crate ([#444](https://github.com/paradigmxyz/solar/issues/444))
- Release 0.1.6 ([#443](https://github.com/paradigmxyz/solar/issues/443))
- Downgrade some debug spans to trace ([#412](https://github.com/paradigmxyz/solar/issues/412))
- Abstract implementation of Declarations ([#410](https://github.com/paradigmxyz/solar/issues/410))
- Remove query tracing ([#406](https://github.com/paradigmxyz/solar/issues/406))
- Clean up contract inheritance linearization ([#405](https://github.com/paradigmxyz/solar/issues/405))

### Other

- Enforce typos ([#423](https://github.com/paradigmxyz/solar/issues/423))

### Performance

- [interface] Cache thread pool inside of session ([#458](https://github.com/paradigmxyz/solar/issues/458))
- Load input source files in parallel ([#429](https://github.com/paradigmxyz/solar/issues/429))
- [sema] Better parallel parser scheduling ([#428](https://github.com/paradigmxyz/solar/issues/428))
- Pool symbol resolver scopes, refactor ([#413](https://github.com/paradigmxyz/solar/issues/413))
- [sema] Add some reserve calls ([#411](https://github.com/paradigmxyz/solar/issues/411))
- [sema] Use `Cell<usize>` in lowering ([#408](https://github.com/paradigmxyz/solar/issues/408))

## [0.1.5](https://github.com/paradigmxyz/solar/releases/tag/v0.1.5)

### Bug Fixes

- Visit modifiers in hir visitor ([#373](https://github.com/paradigmxyz/solar/issues/373))
- [sema] Don't warn 3628 if no interface functions ([#330](https://github.com/paradigmxyz/solar/issues/330))
- [sema] Declare selfdestruct builtin ([#297](https://github.com/paradigmxyz/solar/issues/297))
- [sema] Correct variable declaration scopes ([#300](https://github.com/paradigmxyz/solar/issues/300))
- Invalid underscores check ([#281](https://github.com/paradigmxyz/solar/issues/281))

### Dependencies

- [deps] Weekly `cargo update` ([#351](https://github.com/paradigmxyz/solar/issues/351))

### Features

- Resolve ctor base args ([#322](https://github.com/paradigmxyz/solar/issues/322))
- Add span visitor debug tool ([#355](https://github.com/paradigmxyz/solar/issues/355))
- [sema] Implement receive function checks ([#321](https://github.com/paradigmxyz/solar/issues/321))
- [sema] Display more types, add Ty::display ([#328](https://github.com/paradigmxyz/solar/issues/328))
- Add span in FunctionHeader ([#318](https://github.com/paradigmxyz/solar/issues/318))
- [ast] Add spans to blocks ([#314](https://github.com/paradigmxyz/solar/issues/314))
- Typecheck for external type clashes ([#312](https://github.com/paradigmxyz/solar/issues/312))
- Warn when payable fallback function found but no receive function ([#170](https://github.com/paradigmxyz/solar/issues/170))
- Literal checks ([#287](https://github.com/paradigmxyz/solar/issues/287))
- [ast] Store concatenated string literals ([#266](https://github.com/paradigmxyz/solar/issues/266))
- [ast] Add span to CallArgs ([#265](https://github.com/paradigmxyz/solar/issues/265))
- Add CompilerStage::ParsedAndImported ([#259](https://github.com/paradigmxyz/solar/issues/259))

### Miscellaneous Tasks

- Release 0.1.5 ([#382](https://github.com/paradigmxyz/solar/issues/382))
- Use Option<StateMutability> in the AST ([#374](https://github.com/paradigmxyz/solar/issues/374))
- Fn header spans ([#371](https://github.com/paradigmxyz/solar/issues/371))
- Misc cleanup, util methods ([#367](https://github.com/paradigmxyz/solar/issues/367))
- Add span to `TryCatchClause` ([#364](https://github.com/paradigmxyz/solar/issues/364))
- [parser] Move unescaping from lexer to parser ([#360](https://github.com/paradigmxyz/solar/issues/360))
- Release 0.1.4 ([#341](https://github.com/paradigmxyz/solar/issues/341))
- Release 0.1.3 ([#310](https://github.com/paradigmxyz/solar/issues/310))
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

### Miscellaneous Tasks

- Release 0.1.2
- Release 0.1.2 ([#251](https://github.com/paradigmxyz/solar/issues/251))

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

- Release 0.1.1 ([#182](https://github.com/paradigmxyz/solar/issues/182))
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
