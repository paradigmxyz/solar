# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0](https://github.com/paradigmxyz/solar/releases/tag/v0.3.0)

> [!WARNING]
> This is the first release with code generation, and it is experimental. Try it
> locally and report bugs, but do not deploy contracts compiled by Solar to
> production yet.

### Highlights

- **Code generation (experimental).** The compiler now turns Solidity into EVM
  bytecode through its own pipeline: typed MIR, then EVM IR, then stack
  scheduling, then the assembler. MIR lowering runs in phases ([#921]), memory
  follows a new model ([#1004]), and functions can be marked cold ([#948]).
  Optimizations are written as ISLE rules over an e-graph, driven by a
  declarative operation schema ([#1373], [#1509]). The Amsterdam EVM version is
  supported ([#1288]). When too many values are live on the stack, the stack
  scheduler spills some to memory instead of failing with "stack too deep"
  ([#1187], [#1368], [#2006], [#2093]). The compiler warns whenever it
  generates code.
- **Optimizer.** `-O gas` (the default) and `-O size` target runtime gas and
  bytecode size ([#1080], [#2045], [#2047]). There is inlining, memory DSE,
  SCCP, LICM and CSE, plus native integer widths ([#1525]). Word rewrite rules
  are proved in Lean 4 against an EVM model ([`2c31d78`]).
- **New IR crates and interpreters.** `solar-mir` and `solar-evmir` hold the IR
  data models and text formats ([#2202], [#1782]). New MIR and EVM IR
  interpreters run lowered code, and the test suite checks it with `run-call`
  directives ([#1759], [#1782], [#1026]).
- **Standard JSON.** Standard JSON input now drives codegen ([#1102]),
  including `settings.debug` ([#1382]), CBOR auxdata positions keyed by
  contract ([#2142], [#2149]), and codegen flags taken only from the input
  ([#2143]). Unknown fields are ignored ([#1574]) and outputs are built in
  parallel ([#1602], [#2023]).
- **Outputs and metadata.** The compiler now emits bytecode metadata
  ([#1344]), NatSpec docs and storage layout ([#933]), debug info for optimized
  code ([#1325]) and debug artifacts ([#1408]). Solar builds are named in
  metadata and version output ([#2141]), and Solar-specific NatSpec tags are
  recognized ([#2101]).
- **Frontend.** Type checking is on by default ([#939]). We ported solc's
  control-flow analyzer ([#1737]), added the view/pure checker ([#980]),
  rewrote constant evaluation ([#1878], [#1896]), support fractional number
  literals ([#1752]) and resolve imports like solc ([#1740]). Many diagnostics
  now carry solc's error codes ([#1810], [#1821], [#1823]). Parser recovery
  handles incomplete input ([#942]).
- **Frontend performance.** Type checking runs in parallel per item ([#2061]).
  Query and attached-function caches are faster ([#1857], [#2109]), parser
  stack frames are smaller ([#1741]), the binary uses mimalloc v2 ([#2129]),
  and release binaries are built with PGO ([#1327]).
- **Language server.** The language server now has completions, signature
  help, rename, hover, call and type hierarchies, CodeLens, pull and workspace
  diagnostics, flychecks, and quick fixes ([#930], [#941], [#950], [#1031],
  [#1035], [#1050], [#1111], [#1130], [#1646]). It reads Foundry configs
  ([#1243], [#1320]), exposes an embedding API ([#1209]) and indexes
  workspaces incrementally ([#1422], [#1447], [#1471]). The VS Code and Zed
  clients now live in Foundry ([#1415]).
- **Linting.** The new `solar-lint` crate provides an extensible lint
  framework ([#1025]), backed by sema query helpers ([#952], [#954], [#955]).
- **solc compatibility.** We track solc 0.8.38 ([#1434], [#2021]). Intended
  differences from solc are recorded in
  [`docs/SOLC_DIVERGENCE.md`](https://github.com/paradigmxyz/solar/blob/main/docs/SOLC_DIVERGENCE.md)
  ([#980]). A new `compiler-diff` tool compares outputs and execution against
  solc ([#1498]). Real Foundry projects run as a differential suite ([#1213],
  [#2046]), and Solar ships in Foundry releases ([#2133]).

### Bug Fixes

- [codegen] Make bytecode independent of unrelated sources ([#2242](https://github.com/paradigmxyz/solar/issues/2242))
- Honor --stop-after for lowering and analysis ([#2243](https://github.com/paradigmxyz/solar/issues/2243))
- [mir] Correct byte-run-loads leaves and shared reads ([#2162](https://github.com/paradigmxyz/solar/issues/2162))
- [codegen] Stop tail-copy cost scans at the budget ([#2197](https://github.com/paradigmxyz/solar/issues/2197))
- [codegen] Remove unused move_terminator ([#2198](https://github.com/paradigmxyz/solar/issues/2198))
- [codegen] Zero the padding of decoded bytes ([#2147](https://github.com/paradigmxyz/solar/issues/2147))
- [config] Honor SOLAR_TEST_VERSION in any profile ([#2144](https://github.com/paradigmxyz/solar/issues/2144))
- [codegen] Protect spills from writes the compiler cannot place ([#2006](https://github.com/paradigmxyz/solar/issues/2006))
- Emit diagnostics in the same order at every thread count ([#2131](https://github.com/paradigmxyz/solar/issues/2131))
- [mir] Keep storage alias bases current and reject statements after a terminator ([#2135](https://github.com/paradigmxyz/solar/issues/2135))
- [codegen] Validate calldata arrays where read ([#2078](https://github.com/paradigmxyz/solar/issues/2078))
- [codegen] Copy call input bytes as solc does ([#2085](https://github.com/paradigmxyz/solar/issues/2085))
- [codegen] Zero the word after packed encodings ([#2072](https://github.com/paradigmxyz/solar/issues/2072))
- [mir] Inline assembly dirty bits in modifiers and immutables ([#2002](https://github.com/paradigmxyz/solar/issues/2002))
- [mir] Ignore forwarded gas in call summaries ([#1920](https://github.com/paradigmxyz/solar/issues/1920))
- [codegen] Harden recovery and calldata ([#1901](https://github.com/paradigmxyz/solar/issues/1901))
- [codegen] Bind call results across reads ([#1908](https://github.com/paradigmxyz/solar/issues/1908))
- [codegen] Distinguish control-flow costs ([#1906](https://github.com/paradigmxyz/solar/issues/1906))
- [sema] Reject every unfolded constant in asm ([#1900](https://github.com/paradigmxyz/solar/issues/1900))
- [sema] Reject out-of-range literal indexes ([#1898](https://github.com/paradigmxyz/solar/issues/1898))
- [codegen] Use calldata access checks on returns ([#1876](https://github.com/paradigmxyz/solar/issues/1876))
- [codegen] Lower storage reference conversions ([#1870](https://github.com/paradigmxyz/solar/issues/1870))
- [sema] Drop cached output allocations again ([#1889](https://github.com/paradigmxyz/solar/issues/1889))
- [parse] Validate assembly dialect and flag strings ([#1882](https://github.com/paradigmxyz/solar/issues/1882))
- [parse] Reject `address payable` mapping keys ([#1883](https://github.com/paradigmxyz/solar/issues/1883))
- [cli] Free the compiler in library calls ([#1884](https://github.com/paradigmxyz/solar/issues/1884))
- [parse] Require names in variable declarations ([#1885](https://github.com/paradigmxyz/solar/issues/1885))
- [parse] Reject unclosed sequences at EOF ([#1887](https://github.com/paradigmxyz/solar/issues/1887))
- [codegen] Lower Yul multi-assignments to members ([#1873](https://github.com/paradigmxyz/solar/issues/1873))
- [capi] Free the compiler after each compile ([#1874](https://github.com/paradigmxyz/solar/issues/1874))
- [sema] Skip version check for broken pragmas ([#1871](https://github.com/paradigmxyz/solar/issues/1871))
- [sema] Skip version check on bad pragmas ([#1869](https://github.com/paradigmxyz/solar/issues/1869))
- [interface] Strip file:// in callback reads ([#1860](https://github.com/paradigmxyz/solar/issues/1860))
- [parse] Limit recursion in types and `**` chains ([#1865](https://github.com/paradigmxyz/solar/issues/1865))
- [mir] Evaluate abi.encodeCall function receiver ([#1863](https://github.com/paradigmxyz/solar/issues/1863))
- [sema] Keep UDVT names in mapping signatures ([#1856](https://github.com/paradigmxyz/solar/issues/1856))
- [mir] Bound narrow counters in check-elim ([#1859](https://github.com/paradigmxyz/solar/issues/1859))
- [sema] Stop ABI recursion on recursive structs ([#1853](https://github.com/paradigmxyz/solar/issues/1853))
- [interface] Resolve imports like solc ([#1740](https://github.com/paradigmxyz/solar/issues/1740))
- [parse] Stop pragma versions at the first error ([#1854](https://github.com/paradigmxyz/solar/issues/1854))
- [sema] Report Yul reads of Solidity locals ([#1798](https://github.com/paradigmxyz/solar/issues/1798))
- [sema] Scope function type parameter names ([#1835](https://github.com/paradigmxyz/solar/issues/1835))
- [sema] Report direct struct cycles next to arrays ([#1819](https://github.com/paradigmxyz/solar/issues/1819))
- [sema] Size memory types as solc does ([#1830](https://github.com/paradigmxyz/solar/issues/1830))
- [sema] Reject payable internal function types ([#1803](https://github.com/paradigmxyz/solar/issues/1803))
- [parse] Use string token span for unescaping ([#1851](https://github.com/paradigmxyz/solar/issues/1851))
- [parse] Reject unbalanced bidi override markers ([#1852](https://github.com/paradigmxyz/solar/issues/1852))
- [sema] Reject literals without a mobile type ([#1832](https://github.com/paradigmxyz/solar/issues/1832))
- [sema] Type-check storage layout base slots ([#1775](https://github.com/paradigmxyz/solar/issues/1775))
- [sema] Stop folding bytesN constants ([#1784](https://github.com/paradigmxyz/solar/issues/1784))
- [sema] Report reserved names like solc ([#1768](https://github.com/paradigmxyz/solar/issues/1768))
- [sema] Reject delete on storage pointers ([#1822](https://github.com/paradigmxyz/solar/issues/1822))
- [codegen] Revert on oversized memory defaults ([#1827](https://github.com/paradigmxyz/solar/issues/1827))
- [parse] Treat denominated hex addresses as numbers ([#1829](https://github.com/paradigmxyz/solar/issues/1829))
- [sema] Validate struct field types ([#1809](https://github.com/paradigmxyz/solar/issues/1809))
- [sema] Hide getters behind foreign contract names ([#1777](https://github.com/paradigmxyz/solar/issues/1777))
- [sema] Skip function types in struct recursion ([#1790](https://github.com/paradigmxyz/solar/issues/1790))
- [sema] Add solc codes to Yul reference checks ([#1793](https://github.com/paradigmxyz/solar/issues/1793))
- [sema] Reject invalid function type visibility ([#1795](https://github.com/paradigmxyz/solar/issues/1795))
- [sema] Add solc codes to operator errors ([#1797](https://github.com/paradigmxyz/solar/issues/1797))
- [sema] Report duplicate enum values ([#1801](https://github.com/paradigmxyz/solar/issues/1801))
- [codegen] Cap lazy ABI args behind static arrays ([#1804](https://github.com/paradigmxyz/solar/issues/1804))
- [sema] Skip Yul reference checks on errors ([#1806](https://github.com/paradigmxyz/solar/issues/1806))
- [sema] Reject events that share a function name ([#1807](https://github.com/paradigmxyz/solar/issues/1807))
- [sema] Give declaration conflicts solc's codes ([#1812](https://github.com/paradigmxyz/solar/issues/1812))
- [sema] Reject delete on mappings ([#1816](https://github.com/paradigmxyz/solar/issues/1816))
- [sema] Reject named arguments in conversions ([#1828](https://github.com/paradigmxyz/solar/issues/1828))
- [sema] Reject inheriting a custom storage layout ([#1818](https://github.com/paradigmxyz/solar/issues/1818))
- [sema] Reject unencodable function values ([#1820](https://github.com/paradigmxyz/solar/issues/1820))
- [lsp] Discover nested Foundry projects ([#1780](https://github.com/paradigmxyz/solar/issues/1780))
- [lsp] Resolve nested Foundry remappings ([#1642](https://github.com/paradigmxyz/solar/issues/1642))
- [sema] Reject struct comparisons ([#1787](https://github.com/paradigmxyz/solar/issues/1787))
- [sema] Reject leave outside Yul functions ([#1788](https://github.com/paradigmxyz/solar/issues/1788))
- [sema] Reject duplicate Yul declarations ([#1789](https://github.com/paradigmxyz/solar/issues/1789))
- Accept paren errors and module ternaries ([#1779](https://github.com/paradigmxyz/solar/issues/1779))
- [sema] Check special function signatures ([#1767](https://github.com/paradigmxyz/solar/issues/1767))
- [sema] Reject library names as types and bases ([#1772](https://github.com/paradigmxyz/solar/issues/1772))
- [sema] Stop error cascades from bad operands ([#1764](https://github.com/paradigmxyz/solar/issues/1764))
- [sema] Skip explicit casts of error operands ([#1762](https://github.com/paradigmxyz/solar/issues/1762))
- [codegen] Left-align folded bytesN constants ([#1753](https://github.com/paradigmxyz/solar/issues/1753))
- [codegen] Compile Seaport's assembly correctly ([#1699](https://github.com/paradigmxyz/solar/issues/1699))
- [sema] Reject Yul break outside for loops ([#1747](https://github.com/paradigmxyz/solar/issues/1747))
- [parser] Raise the recursion limit to 256 ([#1704](https://github.com/paradigmxyz/solar/issues/1704))
- [interface] Use `/` in resolved names ([#1736](https://github.com/paradigmxyz/solar/issues/1736))
- [codegen] Bind storage params of inlined ctors ([#1723](https://github.com/paradigmxyz/solar/issues/1723))
- [codegen] Copy storage ternary branches once ([#1705](https://github.com/paradigmxyz/solar/issues/1705))
- [codegen] Bind unassigned storage references ([#1721](https://github.com/paradigmxyz/solar/issues/1721))
- [cli] Print source names verbatim ([#1702](https://github.com/paradigmxyz/solar/issues/1702))
- [codegen] Copy returned calldata aggregate arrays ([#1726](https://github.com/paradigmxyz/solar/issues/1726))
- [codegen] Lower struct fields named offset ([#1716](https://github.com/paradigmxyz/solar/issues/1716))
- [sema] Check function type parameter locations ([#1711](https://github.com/paradigmxyz/solar/issues/1711))
- [parse] Parse `fixedMxN` and `ufixedMxN` types ([#1724](https://github.com/paradigmxyz/solar/issues/1724))
- [interface] Name callback imports by source unit ([#1731](https://github.com/paradigmxyz/solar/issues/1731))
- [codegen] Evaluate discarded constant references ([#1713](https://github.com/paradigmxyz/solar/issues/1713))
- [codegen] Lower abi.decode with empty targets ([#1725](https://github.com/paradigmxyz/solar/issues/1725))
- [sema] Ignore underscores in address checksums ([#1715](https://github.com/paradigmxyz/solar/issues/1715))
- [codegen] Lower conversions to path-typed arrays ([#1720](https://github.com/paradigmxyz/solar/issues/1720))
- [codegen] Try external pointers in struct fields ([#1727](https://github.com/paradigmxyz/solar/issues/1727))
- [codegen] Size block-local liveness rows ([#1719](https://github.com/paradigmxyz/solar/issues/1719))
- [sema] Type `~` on integer literals by value ([#1722](https://github.com/paradigmxyz/solar/issues/1722))
- [cli] Keep backslashes in standard JSON names ([#1728](https://github.com/paradigmxyz/solar/issues/1728))
- [sema] Allow underscores around uppercase `E` ([#1718](https://github.com/paradigmxyz/solar/issues/1718))
- [sema] Declare module aliases before imports ([#1717](https://github.com/paradigmxyz/solar/issues/1717))
- [sema] Resolve qualified emits to own members ([#1714](https://github.com/paradigmxyz/solar/issues/1714))
- [interface] Resolve relative base paths ([#1706](https://github.com/paradigmxyz/solar/issues/1706))
- [codegen] Skip bare reference statements ([#1712](https://github.com/paradigmxyz/solar/issues/1712))
- [interface] Keep Standard JSON source keys ([#1709](https://github.com/paradigmxyz/solar/issues/1709))
- [sema] Resolve Yul calls only to Yul names ([#1710](https://github.com/paradigmxyz/solar/issues/1710))
- [codegen] Use sema type for conditionals ([#1708](https://github.com/paradigmxyz/solar/issues/1708))
- [sema] Keep string literal length in its type ([#1707](https://github.com/paradigmxyz/solar/issues/1707))
- [mir] Refresh counter entry in indvar-simplify ([#1703](https://github.com/paradigmxyz/solar/issues/1703))
- [mir] Bound computed-address alias assumptions ([#1695](https://github.com/paradigmxyz/solar/issues/1695))
- [codegen] Check values live across calls ([#1694](https://github.com/paradigmxyz/solar/issues/1694))
- [codegen] Preserve paired phi stack entries ([#1696](https://github.com/paradigmxyz/solar/issues/1696))
- [codegen] Lower storage assignment values ([#1681](https://github.com/paradigmxyz/solar/issues/1681))
- [codegen] Use full-width storage array layouts ([#1687](https://github.com/paradigmxyz/solar/issues/1687))
- [codegen] Materialize calldata tuple values ([#1686](https://github.com/paradigmxyz/solar/issues/1686))
- [codegen] Retain ABI array tail provenance ([#1688](https://github.com/paradigmxyz/solar/issues/1688))
- [codegen] Check merged branch stack depth ([#1689](https://github.com/paradigmxyz/solar/issues/1689))
- [codegen] Retain initialized storage references ([#1683](https://github.com/paradigmxyz/solar/issues/1683))
- [codegen] Lower mapping values as slots ([#1680](https://github.com/paradigmxyz/solar/issues/1680))
- [codegen] Resolve qualified storage accesses ([#1682](https://github.com/paradigmxyz/solar/issues/1682))
- [codegen] Preserve loop exit dominance ([#1671](https://github.com/paradigmxyz/solar/issues/1671))
- [codegen] Evaluate discarded receivers ([#1674](https://github.com/paradigmxyz/solar/issues/1674))
- [codegen] Rematerialize missing cast operands ([#1676](https://github.com/paradigmxyz/solar/issues/1676))
- [codegen] Lower unit-valued return expressions ([#1670](https://github.com/paradigmxyz/solar/issues/1670))
- [codegen] Lower attached calls on this ([#1673](https://github.com/paradigmxyz/solar/issues/1673))
- [codegen] Widen Yul switch selectors ([#1675](https://github.com/paradigmxyz/solar/issues/1675))
- [codegen] Preserve memory ternary values ([#1667](https://github.com/paradigmxyz/solar/issues/1667))
- [codegen] Lower UDVT identity conversions ([#1668](https://github.com/paradigmxyz/solar/issues/1668))
- [tests] Validate IR and correct fuzz assumptions ([#1666](https://github.com/paradigmxyz/solar/issues/1666))
- [lsp] Skip completion in comments ([#1656](https://github.com/paradigmxyz/solar/issues/1656))
- [lsp] Handle standalone CR locations ([#1657](https://github.com/paradigmxyz/solar/issues/1657))
- [codegen] Keep stores before halting calls ([#1655](https://github.com/paradigmxyz/solar/issues/1655))
- [lsp] Complete members across line breaks ([#1643](https://github.com/paradigmxyz/solar/issues/1643))
- [lsp] Fall back to declarations ([#1645](https://github.com/paradigmxyz/solar/issues/1645))
- [codegen] Let resident layouts use full DUP reach ([#1641](https://github.com/paradigmxyz/solar/issues/1641))
- [lsp] Index implicit constructor calls ([#1640](https://github.com/paradigmxyz/solar/issues/1640))
- [lsp] Preserve valid imports on file moves ([#1639](https://github.com/paradigmxyz/solar/issues/1639))
- [lsp] Rename named args with call options ([#1637](https://github.com/paradigmxyz/solar/issues/1637))
- [lsp] Protect nested repositories on rename ([#1596](https://github.com/paradigmxyz/solar/issues/1596))
- [interface] Resolve absolute remapping targets to base-relative source units ([#1628](https://github.com/paradigmxyz/solar/issues/1628))
- [lsp] Protect dependencies in file operations ([#1593](https://github.com/paradigmxyz/solar/issues/1593))
- [lsp] Preserve diagnostic details and tags ([#1597](https://github.com/paradigmxyz/solar/issues/1597))
- [lsp] Resolve qualified path segments ([#1610](https://github.com/paradigmxyz/solar/issues/1610))
- Inherit workspace lints in benchmarks ([#1603](https://github.com/paradigmxyz/solar/issues/1603))
- [lsp] Clarify rename scope errors ([#1594](https://github.com/paradigmxyz/solar/issues/1594))
- [sema] Preserve library ABI type names ([#1590](https://github.com/paradigmxyz/solar/issues/1590))
- [lsp] Reject stale flycheck diagnostics ([#1540](https://github.com/paradigmxyz/solar/issues/1540))
- Preserve empty source output on errors ([#1591](https://github.com/paradigmxyz/solar/issues/1591))
- [cli] Emit requested empty method identifiers ([#1589](https://github.com/paradigmxyz/solar/issues/1589))
- Increase proof test timeout headroom ([#1586](https://github.com/paradigmxyz/solar/issues/1586))
- [parser] Accept extra version components ([#1567](https://github.com/paradigmxyz/solar/issues/1567))
- [parser] Keep Yul builtin name checks ([#1584](https://github.com/paradigmxyz/solar/issues/1584))
- [cli] Ignore unknown standard JSON fields ([#1574](https://github.com/paradigmxyz/solar/issues/1574))
- Match import remappings as string prefixes ([#1576](https://github.com/paradigmxyz/solar/issues/1576))
- [sema] Give tuple assignments the void type ([#1572](https://github.com/paradigmxyz/solar/issues/1572))
- Validate Yul builtin names in sema ([#1568](https://github.com/paradigmxyz/solar/issues/1568))
- [sema] Convert empty strings to fixed bytes ([#1573](https://github.com/paradigmxyz/solar/issues/1573))
- [sema] Exclude external-only unqualified names ([#1577](https://github.com/paradigmxyz/solar/issues/1577))
- Resolve relative source-unit imports ([#1575](https://github.com/paradigmxyz/solar/issues/1575))
- [sema] Resolve named constructor arguments ([#1571](https://github.com/paradigmxyz/solar/issues/1571))
- [sema] Limit qualified contract functions ([#1578](https://github.com/paradigmxyz/solar/issues/1578))
- [lsp] Prevent renaming dependency files ([#1555](https://github.com/paradigmxyz/solar/issues/1555))
- [sema] Exclude Yul functions from overrides ([#1570](https://github.com/paradigmxyz/solar/issues/1570))
- [sema] Lower boolean Yul conditions ([#1569](https://github.com/paradigmxyz/solar/issues/1569))
- [lsp] Reject incomplete workspace renames ([#1554](https://github.com/paradigmxyz/solar/issues/1554))
- Preserve benchmark solc version filtering ([#1565](https://github.com/paradigmxyz/solar/issues/1565))
- [codegen] Repair external Foundry failures ([#1564](https://github.com/paradigmxyz/solar/issues/1564))
- [codegen] Retry inaccessible stack arguments ([#1561](https://github.com/paradigmxyz/solar/issues/1561))
- [codegen] Use target stack reach ([#1559](https://github.com/paradigmxyz/solar/issues/1559))
- [lsp] Canonicalize file URIs ([#1543](https://github.com/paradigmxyz/solar/issues/1543))
- [lsp] Clamp incoming document positions ([#1541](https://github.com/paradigmxyz/solar/issues/1541))
- [lsp] Reject conflicting point query contexts ([#1536](https://github.com/paradigmxyz/solar/issues/1536))
- [sema] Handle abstract IDs and tuple scope ([#1537](https://github.com/paradigmxyz/solar/issues/1537))
- Retain recursive library storage methods ([#1534](https://github.com/paradigmxyz/solar/issues/1534))
- Respect inherited function visibility ([#1531](https://github.com/paradigmxyz/solar/issues/1531))
- Canonicalize library event and error hashes ([#1532](https://github.com/paradigmxyz/solar/issues/1532))
- Evaluate zero to zero as one ([#1533](https://github.com/paradigmxyz/solar/issues/1533))
- Preserve library address relocations ([#1427](https://github.com/paradigmxyz/solar/issues/1427))
- Stabilize LSP benchmarks and naming ([#1502](https://github.com/paradigmxyz/solar/issues/1502))
- [lsp] Reject invalidated analysis requests ([#1496](https://github.com/paradigmxyz/solar/issues/1496))
- [lsp] Complete symbols in incomplete code ([#1475](https://github.com/paradigmxyz/solar/issues/1475))
- [lsp] Index NatSpec symbol references ([#1486](https://github.com/paradigmxyz/solar/issues/1486))
- Exclude runtime IR from creation dumps ([#1448](https://github.com/paradigmxyz/solar/issues/1448))
- Correct ABI data locations and lowering ([#1410](https://github.com/paradigmxyz/solar/issues/1410))
- Normalize Windows CLI usage in UI tests ([#1430](https://github.com/paradigmxyz/solar/issues/1430))
- Repair Windows CI tests ([#1420](https://github.com/paradigmxyz/solar/issues/1420))
- [lsp] Index the whole project workspace ([#1404](https://github.com/paradigmxyz/solar/issues/1404))
- [benches] Update Solar perf permalinks ([#1405](https://github.com/paradigmxyz/solar/issues/1405))
- Stabilize benchmark artifacts and hide stale summaries ([#1403](https://github.com/paradigmxyz/solar/issues/1403))
- Address the review findings of the audit split ([#1383](https://github.com/paradigmxyz/solar/issues/1383))
- [sema] Accept programs solc accepts in the type checker ([#1365](https://github.com/paradigmxyz/solar/issues/1365))
- [sema] Check the try returns clause against the callee ([#1372](https://github.com/paradigmxyz/solar/issues/1372))
- [codegen] Return library storage pointers as slots ([#1369](https://github.com/paradigmxyz/solar/issues/1369))
- [codegen] Match solc's external calls before byzantium ([#1367](https://github.com/paradigmxyz/solar/issues/1367))
- [codegen] Spill across branches and accept recursion ([#1368](https://github.com/paradigmxyz/solar/issues/1368))
- Check and lower base constructor arguments like solc ([#1364](https://github.com/paradigmxyz/solar/issues/1364))
- Fix codegen issues found in foundry tests ([#1361](https://github.com/paradigmxyz/solar/issues/1361))
- [sema] Add solc's interface and free function checks ([#1370](https://github.com/paradigmxyz/solar/issues/1370))
- [sema] Match solc library ABI and declaration checks ([#1363](https://github.com/paradigmxyz/solar/issues/1363))
- [codegen] Remove redundant data clone ([#1354](https://github.com/paradigmxyz/solar/issues/1354))
- [sema] Drop cached output allocations ([#1349](https://github.com/paradigmxyz/solar/issues/1349))
- [sema] Defer ternary branch coercion ([#1338](https://github.com/paradigmxyz/solar/issues/1338))
- [codegen] Bound outline candidates ([#1319](https://github.com/paradigmxyz/solar/issues/1319))
- [zed] Use supported highlight captures ([#1318](https://github.com/paradigmxyz/solar/issues/1318))
- Compile external Foundry projects ([#1314](https://github.com/paradigmxyz/solar/issues/1314))
- [lsp] Gate Unix-only test import ([#1315](https://github.com/paradigmxyz/solar/issues/1315))
- [parser] Match solc syntax errors ([#1308](https://github.com/paradigmxyz/solar/issues/1308))
- [codegen] Legalize legacy EVM shifts ([#1267](https://github.com/paradigmxyz/solar/issues/1267))
- [sema] Reject dynamic calldata slices ([#1303](https://github.com/paradigmxyz/solar/issues/1303))
- [yul] Parse versioned builtin names ([#1284](https://github.com/paradigmxyz/solar/issues/1284))
- Revert accidental Yul EVM version merge
- [yul] Complete EVM version checks
- [sema] Downgrade NatSpec errors to warnings ([#1281](https://github.com/paradigmxyz/solar/issues/1281))
- [sema] Resolve overridden call targets ([#1264](https://github.com/paradigmxyz/solar/issues/1264))
- [sema] Complete EVM version checks ([#1278](https://github.com/paradigmxyz/solar/issues/1278))
- [zed] Improve Solidity highlighting ([#1277](https://github.com/paradigmxyz/solar/issues/1277))
- [sema] Gate high-level EVM features ([#1275](https://github.com/paradigmxyz/solar/issues/1275))
- [codegen] Gate selfbalance optimization ([#1273](https://github.com/paradigmxyz/solar/issues/1273))
- [sema] Match solc fallback and receive overrides ([#1265](https://github.com/paradigmxyz/solar/issues/1265))
- [sema] Allow calldata string slices ([#1256](https://github.com/paradigmxyz/solar/issues/1256))
- [yul] Honor Cancun builtin versions ([#1252](https://github.com/paradigmxyz/solar/issues/1252))
- [parse] Preserve continued string whitespace ([#1211](https://github.com/paradigmxyz/solar/issues/1211))
- [parser] Distinguish underscore identifiers ([#1217](https://github.com/paradigmxyz/solar/issues/1217))
- [codegen] Fix issues detected with seaport codegen ([#1193](https://github.com/paradigmxyz/solar/issues/1193))
- [codegen] Reload reserved spill values ([#1187](https://github.com/paradigmxyz/solar/issues/1187))
- [codegen] Clean unchecked signed division ([#1174](https://github.com/paradigmxyz/solar/issues/1174))
- [codegen] Clean unchecked signed negation ([#1173](https://github.com/paradigmxyz/solar/issues/1173))
- [lsp] Harden import completion ([#1171](https://github.com/paradigmxyz/solar/issues/1171))
- [codegen] Clean narrow bitwise not ([#1146](https://github.com/paradigmxyz/solar/issues/1146))
- [codegen] Mask packed signed fields ([#1151](https://github.com/paradigmxyz/solar/issues/1151))
- [codegen] Truncate narrow left shifts ([#1145](https://github.com/paradigmxyz/solar/issues/1145))
- [codegen] Panic on zero modular builtin ([#1137](https://github.com/paradigmxyz/solar/issues/1137))
- [codegen] Identify dispatch entries ([#1121](https://github.com/paradigmxyz/solar/issues/1121))
- [cli] Correct std json optimizer mapping
- [sema] Include referenced ABI items ([#1103](https://github.com/paradigmxyz/solar/issues/1103))
- [codegen] Remove lowering-time inlining ([#1082](https://github.com/paradigmxyz/solar/issues/1082))
- [sema] Align project edge cases with solc ([#1088](https://github.com/paradigmxyz/solar/issues/1088))
- [codegen] Lower exact base calls ([#1074](https://github.com/paradigmxyz/solar/issues/1074))
- [codegen] Match precompile targets ([#1073](https://github.com/paradigmxyz/solar/issues/1073))
- [codegen] Lower mapping struct fields ([#1070](https://github.com/paradigmxyz/solar/issues/1070))
- [codegen] Order base constructors ([#1067](https://github.com/paradigmxyz/solar/issues/1067))
- [codegen] Lower tuple values ([#1071](https://github.com/paradigmxyz/solar/issues/1071))
- [ast] Reject unchecked as bare loop/if body ([#1064](https://github.com/paradigmxyz/solar/issues/1064))
- [codegen] Resolve inherited calls ([#1068](https://github.com/paradigmxyz/solar/issues/1068))
- [sema] Include library mapping params in hashes ([#1061](https://github.com/paradigmxyz/solar/issues/1061))
- [sema] Reject internal types in external fn types ([#1063](https://github.com/paradigmxyz/solar/issues/1063))
- [lsp] Add missing symbol hovers ([#1054](https://github.com/paradigmxyz/solar/issues/1054))
- [lsp] Debounce workspace analysis ([#1053](https://github.com/paradigmxyz/solar/issues/1053))
- [lsp] Reject unresolved call sites ([#1052](https://github.com/paradigmxyz/solar/issues/1052))
- [codegen] Distinguish tail call targets ([#1042](https://github.com/paradigmxyz/solar/issues/1042))
- [codegen] Harden optimizer semantics ([#1021](https://github.com/paradigmxyz/solar/issues/1021))
- [codegen] Elide redundant stops ([#995](https://github.com/paradigmxyz/solar/issues/995))
- [lsp] Await latest analysis ([#984](https://github.com/paradigmxyz/solar/issues/984))
- [typeck] Distinguish storage copies ([#974](https://github.com/paradigmxyz/solar/issues/974))
- [typeck] Validate return conversions ([#975](https://github.com/paradigmxyz/solar/issues/975))
- [typeck] Reject non-mobile functions ([#976](https://github.com/paradigmxyz/solar/issues/976))
- [typeck] Tighten explicit conversions ([#977](https://github.com/paradigmxyz/solar/issues/977))
- [lsp] Discover nested Foundry projects ([#970](https://github.com/paradigmxyz/solar/issues/970))
- Address some correctness issues ([#949](https://github.com/paradigmxyz/solar/issues/949))
- [lsp] Report failed flychecks ([#947](https://github.com/paradigmxyz/solar/issues/947))
- [sema] Match solc storage layouts ([#945](https://github.com/paradigmxyz/solar/issues/945))
- [typeck] Type Yul bool literals as words ([#937](https://github.com/paradigmxyz/solar/issues/937))

### Documentation

- Pin README to 0.2.0 and fair LibString benchmark ([#2153](https://github.com/paradigmxyz/solar/issues/2153))
- Update README with codegen benchmarks ([#2146](https://github.com/paradigmxyz/solar/issues/2146))
- Mention Solar ships in Foundry releases ([#2133](https://github.com/paradigmxyz/solar/issues/2133))
- Record skipped inline assembly checks ([#1773](https://github.com/paradigmxyz/solar/issues/1773))
- Correct the ABI coder v1 divergence entry ([#1786](https://github.com/paradigmxyz/solar/issues/1786))
- Shrink agent base context ([#1701](https://github.com/paradigmxyz/solar/issues/1701))
- Sync Rust agent guidance ([#1538](https://github.com/paradigmxyz/solar/issues/1538))
- Document call-option function values ([#1272](https://github.com/paradigmxyz/solar/issues/1272))
- Update error annotations

### Features

- Add the MIR interpreter and run-call check ([#1759](https://github.com/paradigmxyz/solar/issues/1759))
- [ir] Comment function entries and block predecessors in text dumps ([#2229](https://github.com/paradigmxyz/solar/issues/2229))
- Add the solar NatSpec tags ([#2101](https://github.com/paradigmxyz/solar/issues/2101))
- [standard-json] Key cborAuxdata by contract ([#2149](https://github.com/paradigmxyz/solar/issues/2149))
- Add -Ztune=jit ([#2152](https://github.com/paradigmxyz/solar/issues/2152))
- [standard-json] Take codegen flags only from input ([#2143](https://github.com/paradigmxyz/solar/issues/2143))
- [standard-json] Report CBOR auxdata positions ([#2142](https://github.com/paradigmxyz/solar/issues/2142))
- Name Solar builds in metadata and version output ([#2141](https://github.com/paradigmxyz/solar/issues/2141))
- Add solar-evmir and an EVM IR interpreter ([#1782](https://github.com/paradigmxyz/solar/issues/1782))
- [codegen] Add -Ztune for execution engines ([#2064](https://github.com/paradigmxyz/solar/issues/2064))
- [codegen] Trace ISLE rule firings ([#2059](https://github.com/paradigmxyz/solar/issues/2059))
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
- [mir] Optimize native Solidity integers ([#1525](https://github.com/paradigmxyz/solar/issues/1525))
- Switch to Lean EVM Rules
- [lsp] Add builtin hover ([#1646](https://github.com/paradigmxyz/solar/issues/1646))
- Unify compiler comparison workflows ([#1498](https://github.com/paradigmxyz/solar/issues/1498))
- [lsp] Make source debounce configurable ([#1511](https://github.com/paradigmxyz/solar/issues/1511))
- [codegen] Improve safe solady ([#1444](https://github.com/paradigmxyz/solar/issues/1444))
- [mir] Enforce semantic lowering boundaries ([#1392](https://github.com/paradigmxyz/solar/issues/1392))
- Capture solx LLVM IR benchmark artifacts ([#1500](https://github.com/paradigmxyz/solar/issues/1500))
- Add local Sourcify corpus runner ([#1480](https://github.com/paradigmxyz/solar/issues/1480))
- Expose rooted struct storage layouts ([#1413](https://github.com/paradigmxyz/solar/issues/1413))
- [cli] Expose debug artifact outputs ([#1408](https://github.com/paradigmxyz/solar/issues/1408))
- [lsp] Add cross-server benchmark ([#1326](https://github.com/paradigmxyz/solar/issues/1326))
- Compare codegen benchmark runs ([#1394](https://github.com/paradigmxyz/solar/issues/1394))
- [codegen] Guard non-view library functions ([#1385](https://github.com/paradigmxyz/solar/issues/1385))
- Support Standard JSON `settings.debug` ([#1382](https://github.com/paradigmxyz/solar/issues/1382))
- Audit and emit optimized debug info ([#1325](https://github.com/paradigmxyz/solar/issues/1325))
- [lsp] Reload host Foundry config ([#1320](https://github.com/paradigmxyz/solar/issues/1320))
- [cli] Add bytecode metadata ([#1344](https://github.com/paradigmxyz/solar/issues/1344))
- [codegen] Add shared program data ([#1304](https://github.com/paradigmxyz/solar/issues/1304))
- Build release binaries with PGO ([#1327](https://github.com/paradigmxyz/solar/issues/1327))
- [cli] Highlight IR console output ([#1317](https://github.com/paradigmxyz/solar/issues/1317))
- [codegen] Implement Amsterdam EVM version ([#1288](https://github.com/paradigmxyz/solar/issues/1288))
- [lsp] Accept resolved Foundry configs ([#1243](https://github.com/paradigmxyz/solar/issues/1243))
-  feat(lsp): propagate selected Foundry profile ([#1228](https://github.com/paradigmxyz/solar/issues/1228))
- [lsp] Add embeddable launch API ([#1209](https://github.com/paradigmxyz/solar/issues/1209))
- [lsp] Add on-demand PR regression benchmark ([#1195](https://github.com/paradigmxyz/solar/issues/1195))
- [lsp] Bound workspace indexing ([#1139](https://github.com/paradigmxyz/solar/issues/1139))
- [cli] Warn instead of -Zcodegen ([#1169](https://github.com/paradigmxyz/solar/issues/1169))
- [lsp] Enforce protocol compatibility ([#1159](https://github.com/paradigmxyz/solar/issues/1159))
- [lsp] Add import resolution ([#1152](https://github.com/paradigmxyz/solar/issues/1152))
- [codegen] Add late dce ([#1140](https://github.com/paradigmxyz/solar/issues/1140))
- [lsp] Add diagnostic quick fixes ([#1130](https://github.com/paradigmxyz/solar/issues/1130))
- [lsp] Add protocol tracing ([#1124](https://github.com/paradigmxyz/solar/issues/1124))
- [lsp] Add workspace diagnostics ([#1111](https://github.com/paradigmxyz/solar/issues/1111))
- Inline large shared callees ([#1108](https://github.com/paradigmxyz/solar/issues/1108))
- Elide discarded store copies ([#1107](https://github.com/paradigmxyz/solar/issues/1107))
- Bulk memory zero ([#1096](https://github.com/paradigmxyz/solar/issues/1096))
- [cli] Expand standard JSON codegen support ([#1102](https://github.com/paradigmxyz/solar/issues/1102))
- Bulk zero return structs ([#1095](https://github.com/paradigmxyz/solar/issues/1095))
- [codegen] Support narrow immutables ([#1003](https://github.com/paradigmxyz/solar/issues/1003))
- [codegen] Mangle MIR function symbols ([#1087](https://github.com/paradigmxyz/solar/issues/1087))
- [lsp] Resolve completion items ([#1094](https://github.com/paradigmxyz/solar/issues/1094))
- Enable more codegen benches ([#1092](https://github.com/paradigmxyz/solar/issues/1092))
- [lsp] Handle workspace file operations ([#1076](https://github.com/paradigmxyz/solar/issues/1076))
- [lsp] Refresh pull results ([#1083](https://github.com/paradigmxyz/solar/issues/1083))
- [codegen] Add disassembly dumps ([#1077](https://github.com/paradigmxyz/solar/issues/1077))
- [cli] Merge IR optimization commands ([#1080](https://github.com/paradigmxyz/solar/issues/1080))
- [codegen] Lower concat builtins ([#1066](https://github.com/paradigmxyz/solar/issues/1066))
- [codegen] Lower crypto precompiles ([#1065](https://github.com/paradigmxyz/solar/issues/1065))
- [lsp] Add CodeLens support ([#1050](https://github.com/paradigmxyz/solar/issues/1050))
- [codegen] Call internal function pointers ([#1039](https://github.com/paradigmxyz/solar/issues/1039))
- [codegen] Simplify clz expressions ([#1048](https://github.com/paradigmxyz/solar/issues/1048))
- [codegen] Lower clz and callcode ([#1046](https://github.com/paradigmxyz/solar/issues/1046))
- [lsp] Add call hierarchy support ([#1035](https://github.com/paradigmxyz/solar/issues/1035))
- [lsp] Add type hierarchy support ([#1031](https://github.com/paradigmxyz/solar/issues/1031))
- [codegen] Support storage array push ([#1033](https://github.com/paradigmxyz/solar/issues/1033))
- [lint] Add extensible lint framework ([#1025](https://github.com/paradigmxyz/solar/issues/1025))
- [lsp] Add folding ranges ([#1024](https://github.com/paradigmxyz/solar/issues/1024))
- [lsp] Report workspace indexing progress ([#1016](https://github.com/paradigmxyz/solar/issues/1016))
- Enable codegen benches ([#1020](https://github.com/paradigmxyz/solar/issues/1020))
- [codegen] Analysis cache framework ([#1011](https://github.com/paradigmxyz/solar/issues/1011))
- [lsp] Add pull diagnostics ([#1010](https://github.com/paradigmxyz/solar/issues/1010))
- [lsp] Add selection range support ([#1012](https://github.com/paradigmxyz/solar/issues/1012))
- Introduce new memory model ([#1004](https://github.com/paradigmxyz/solar/issues/1004))
- [lsp] Add will-save notifications ([#1007](https://github.com/paradigmxyz/solar/issues/1007))
- [codegen] Print pass diffs ([#1001](https://github.com/paradigmxyz/solar/issues/1001))
- [lsp] Add NatSpec completions ([#997](https://github.com/paradigmxyz/solar/issues/997))
- [lsp] Add project benchmark framework ([#993](https://github.com/paradigmxyz/solar/issues/993))
- [lsp] Add analysis cache commands ([#996](https://github.com/paradigmxyz/solar/issues/996))
-  feat(lsp): add hover support ([#987](https://github.com/paradigmxyz/solar/issues/987))
- [lsp] Add go to implementation ([#981](https://github.com/paradigmxyz/solar/issues/981))
- [lsp] Add document highlights ([#986](https://github.com/paradigmxyz/solar/issues/986))
- View/pure checker ([#980](https://github.com/paradigmxyz/solar/issues/980))
- [lsp] Add go to type definition ([#983](https://github.com/paradigmxyz/solar/issues/983))
- [lsp] Add document links ([#969](https://github.com/paradigmxyz/solar/issues/969))
- [sema] Add call argument query ([#955](https://github.com/paradigmxyz/solar/issues/955))
- [sema] Add lint query helpers ([#954](https://github.com/paradigmxyz/solar/issues/954))
- [lsp] Add semantic rename support ([#950](https://github.com/paradigmxyz/solar/issues/950))
- [lsp] Add document formatting ([#956](https://github.com/paradigmxyz/solar/issues/956))
- [codegen] Add pass timing flag ([#962](https://github.com/paradigmxyz/solar/issues/962))
- [codegen] Emit final EVM IR ([#959](https://github.com/paradigmxyz/solar/issues/959))
- [sema] Add literal eval query ([#952](https://github.com/paradigmxyz/solar/issues/952))
- Introduce cold functions ([#948](https://github.com/paradigmxyz/solar/issues/948))
- [sema] Add const value accessors ([#953](https://github.com/paradigmxyz/solar/issues/953))
- Several code size improvements ([#929](https://github.com/paradigmxyz/solar/issues/929))
- [lsp] Add signature help ([#941](https://github.com/paradigmxyz/solar/issues/941))
- [parser] Recover incomplete input ([#942](https://github.com/paradigmxyz/solar/issues/942))
- [sema] Enable type checking by default ([#939](https://github.com/paradigmxyz/solar/issues/939))
- [cli] Color standard JSON diagnostics ([#936](https://github.com/paradigmxyz/solar/issues/936))
- [codegen] Phased MIR lowering and a leaner backend ([#921](https://github.com/paradigmxyz/solar/issues/921))
- [cli] Emit docs and storage layout ([#933](https://github.com/paradigmxyz/solar/issues/933))
- [lsp] Add flychecks ([#930](https://github.com/paradigmxyz/solar/issues/930))

### Performance

- [codegen] Skip zero fills of untouched heap ([#2236](https://github.com/paradigmxyz/solar/issues/2236))
- [codegen] Treat cold code as bytes ([#2230](https://github.com/paradigmxyz/solar/issues/2230))
- [codegen] Join ABI range checks into one abort test ([#2205](https://github.com/paradigmxyz/solar/issues/2205))
- [codegen] Clear low-bit masks with shift pairs ([#2212](https://github.com/paradigmxyz/solar/issues/2212))
- [mir] Test checked power-of-two products by their high bits ([#2204](https://github.com/paradigmxyz/solar/issues/2204))
- [codegen] Price cold pushes like a size build ([#2216](https://github.com/paradigmxyz/solar/issues/2216))
- [codegen] Fold pointer-loop distances in loop-unroll ([#2209](https://github.com/paradigmxyz/solar/issues/2209))
- [codegen] Push zero as RETURNDATASIZE before PUSH0 ([#2207](https://github.com/paradigmxyz/solar/issues/2207))
- [codegen] Flip product-check branches in gas mode ([#2218](https://github.com/paradigmxyz/solar/issues/2218))
- [codegen] Materialize any shifted all-ones mask by shifting ([#2206](https://github.com/paradigmxyz/solar/issues/2206))
- [codegen] Rewrite push c, swap1, sub as add of -c ([#2214](https://github.com/paradigmxyz/solar/issues/2214))
- [codegen] Turn branches into the next block around ([#2210](https://github.com/paradigmxyz/solar/issues/2210))
- [codegen] Mark continuations and failure loops cold ([#2217](https://github.com/paradigmxyz/solar/issues/2217))
- [codegen] Compare against MAX through the complement ([#2219](https://github.com/paradigmxyz/solar/issues/2219))
- [codegen] Propagate edge equalities into memory operands ([#2138](https://github.com/paradigmxyz/solar/issues/2138))
- [mir] Cheaper access proofs and local memory summaries ([#2177](https://github.com/paradigmxyz/solar/issues/2177))
- [mir] Drop dead scratch and pointer stores ([#2095](https://github.com/paradigmxyz/solar/issues/2095))
- [codegen] Narrow heap-floor's clamps ([#2145](https://github.com/paradigmxyz/solar/issues/2145))
- [codegen] Fold decoded array validation and event literals ([#2098](https://github.com/paradigmxyz/solar/issues/2098))
- [evm] Move constants through swaps in peepholes ([#2148](https://github.com/paradigmxyz/solar/issues/2148))
- [evm] Compact pushes and lay out blocks at -O none ([#2150](https://github.com/paradigmxyz/solar/issues/2150))
- [evm] Bound call continuations for compact-pushes ([#2120](https://github.com/paradigmxyz/solar/issues/2120))
- [codegen] Run the late memory-dse under -Osize ([#2123](https://github.com/paradigmxyz/solar/issues/2123))
- [codegen] Share the mcopy helper across memory objects ([#2113](https://github.com/paradigmxyz/solar/issues/2113))
- [codegen] Cut the cost of heap-floor's analysis ([#2140](https://github.com/paradigmxyz/solar/issues/2140))
- [mir] Version loops on lengths and fences ([#2100](https://github.com/paradigmxyz/solar/issues/2100))
- [mir] Keep earlier allocation words across unbounded writes ([#2134](https://github.com/paradigmxyz/solar/issues/2134))
- [mir] Keep unique allocation words across low-memory writes ([#2080](https://github.com/paradigmxyz/solar/issues/2080))
- [mir] Turn byte loops into word operations ([#2099](https://github.com/paradigmxyz/solar/issues/2099))
- [codegen] Price constant keccak folding in memory-dse ([#2111](https://github.com/paradigmxyz/solar/issues/2111))
- [codegen] Rebuild values instead of spilling ([#2093](https://github.com/paradigmxyz/solar/issues/2093))
- [mir] Drop high masks proved by a bytesN guard ([#2115](https://github.com/paradigmxyz/solar/issues/2115))
- [mir] Run egraph after late CSE ([#2114](https://github.com/paradigmxyz/solar/issues/2114))
- [codegen] Run two cheap cleanups at -O none ([#2136](https://github.com/paradigmxyz/solar/issues/2136))
- [codegen] Speed up stack operand preparation ([#2117](https://github.com/paradigmxyz/solar/issues/2117))
- [mir] List scoped relations in check elimination ([#2116](https://github.com/paradigmxyz/solar/issues/2116))
- [codegen] Index mask uses in word-sequence ([#2108](https://github.com/paradigmxyz/solar/issues/2108))
- [codegen] Merge idom chains for many-predecessor blocks ([#2107](https://github.com/paradigmxyz/solar/issues/2107))
- [codegen] Bound early-scheduled contracts to the thread count ([#2130](https://github.com/paradigmxyz/solar/issues/2130))
- [sema] Cache attached functions per type and scope ([#2109](https://github.com/paradigmxyz/solar/issues/2109))
- [codegen] Free the stack plan cache after code generation ([#2128](https://github.com/paradigmxyz/solar/issues/2128))
- [cli] Use mimalloc v2 ([#2129](https://github.com/paradigmxyz/solar/issues/2129))
- [evm] Retain several constants on the stack ([#2122](https://github.com/paradigmxyz/solar/issues/2122))
- [mir] Forward stored values past writes at a fresh fmp read ([#2034](https://github.com/paradigmxyz/solar/issues/2034))
- [codegen] Forward zero words from memory_zero in CSE ([#2075](https://github.com/paradigmxyz/solar/issues/2075))
- [evm] Price compact constants by their gas ([#2097](https://github.com/paradigmxyz/solar/issues/2097))
- [codegen] Price a join's wanted order ([#2106](https://github.com/paradigmxyz/solar/issues/2106))
- [codegen] Remove dead stores into allocations ([#2068](https://github.com/paradigmxyz/solar/issues/2068))
- [codegen] Try exchanged loop header layouts ([#2083](https://github.com/paradigmxyz/solar/issues/2083))
- [evm] Tighten dispatch and terminal cleanup ([#1904](https://github.com/paradigmxyz/solar/issues/1904))
- [mir] Bound more values in check elimination ([#2094](https://github.com/paradigmxyz/solar/issues/2094))
- [mir] Add word identities for checked code ([#2086](https://github.com/paradigmxyz/solar/issues/2086))
- [mir] Price and simplify if-conversion ([#2087](https://github.com/paradigmxyz/solar/issues/2087))
- [mir] Hoist array header offsets out of loops ([#2090](https://github.com/paradigmxyz/solar/issues/2090))
- [mir] Forward stores across equal slot hashes ([#2092](https://github.com/paradigmxyz/solar/issues/2092))
- [mir] Read clean address elements unmasked ([#2089](https://github.com/paradigmxyz/solar/issues/2089))
- [evm] Copy small loop joins in gas builds ([#2096](https://github.com/paradigmxyz/solar/issues/2096))
- [codegen] Encode calldata aggregates from calldata ([#2049](https://github.com/paradigmxyz/solar/issues/2049))
- [inline] Consume multi-site leaves in -Osize ([#2065](https://github.com/paradigmxyz/solar/issues/2065))
- [codegen] Stop packing wide jump table entries ([#2066](https://github.com/paradigmxyz/solar/issues/2066))
- [codegen] Keep loop header branches in size mode ([#2067](https://github.com/paradigmxyz/solar/issues/2067))
- [codegen] Run required lowering per function in parallel ([#2058](https://github.com/paradigmxyz/solar/issues/2058))
- [codegen] Reduce loop product checks in -Osize ([#2062](https://github.com/paradigmxyz/solar/issues/2062))
- [sema] Parallelize type checking at item grain ([#2061](https://github.com/paradigmxyz/solar/issues/2061))
- [config] Skip git HEAD tracking in debug builds ([#2055](https://github.com/paradigmxyz/solar/issues/2055))
- [codegen] Lower legacy shifts in MIR ([#2056](https://github.com/paradigmxyz/solar/issues/2056))
- [codegen] Skip mcopy alias analysis at -O none ([#2052](https://github.com/paradigmxyz/solar/issues/2052))
- [mir] Drop mode branches for immutable masks and slot hashes ([#2053](https://github.com/paradigmxyz/solar/issues/2053))
- [mir] Skip full phase validation in release builds ([#2054](https://github.com/paradigmxyz/solar/issues/2054))
- [codegen] Drop size-mode ABI byte tail and literal fold exceptions ([#2050](https://github.com/paradigmxyz/solar/issues/2050))
- [inline] Consume single-call callees in -Osize ([#2051](https://github.com/paradigmxyz/solar/issues/2051))
- [codegen] Delete redundancy passes that measure as zero ([#2037](https://github.com/paradigmxyz/solar/issues/2037))
- [codegen] Fuse dynamic ABI offset range checks ([#2035](https://github.com/paradigmxyz/solar/issues/2035))
- [mir] Fold storage load CSE into cse, dse transient stores ([#2043](https://github.com/paradigmxyz/solar/issues/2043))
- [codegen] Skip stack layout search at -O none ([#2038](https://github.com/paradigmxyz/solar/issues/2038))
- [codegen] Skip repeated lowered MIR check ([#2039](https://github.com/paradigmxyz/solar/issues/2039))
- [codegen] Fold `push 0; lt; iszero` into `iszero` ([#2041](https://github.com/paradigmxyz/solar/issues/2041))
- [codegen] Cheaper heap prefix offsets ([#2044](https://github.com/paradigmxyz/solar/issues/2044))
- [mir] Retune -Osize pass slots ([#2045](https://github.com/paradigmxyz/solar/issues/2045))
- [codegen] Fix four size-mode decisions that grew -Osize output ([#2047](https://github.com/paradigmxyz/solar/issues/2047))
- [codegen] Run MIR function passes on idle workers ([#2017](https://github.com/paradigmxyz/solar/issues/2017))
- [mir] Key frame-constant reads as one operand in CSE ([#1971](https://github.com/paradigmxyz/solar/issues/1971))
- [codegen] Reuse stack plans of identical functions ([#2022](https://github.com/paradigmxyz/solar/issues/2022))
- [codegen] Share stack-reschedule searches across programs ([#2009](https://github.com/paradigmxyz/solar/issues/2009))
- [mir] Compare masked selectors by their top bits ([#1981](https://github.com/paradigmxyz/solar/issues/1981))
- [evm] Place kept words while popping in stack runs ([#1985](https://github.com/paradigmxyz/solar/issues/1985))
- Drop color-eyre from the test build ([#2032](https://github.com/paradigmxyz/solar/issues/2032))
- Make solar-codegen an optional dependency of solar-compiler ([#2028](https://github.com/paradigmxyz/solar/issues/2028))
- [build] Share dev artifacts between workspace and -p builds ([#2025](https://github.com/paradigmxyz/solar/issues/2025))
- [cli] Serialize standard JSON contracts in parallel ([#2023](https://github.com/paradigmxyz/solar/issues/2023))
- [codegen] Plan function stack layouts in parallel ([#2007](https://github.com/paradigmxyz/solar/issues/2007))
- [codegen] Lower the stack planner's constant factors ([#2010](https://github.com/paradigmxyz/solar/issues/2010))
- [codegen] Reuse memory call summaries in inline ([#2011](https://github.com/paradigmxyz/solar/issues/2011))
- [codegen] Make pack-data cheaper ([#2012](https://github.com/paradigmxyz/solar/issues/2012))
- [codegen] Cut check-elim fact setup cost ([#2013](https://github.com/paradigmxyz/solar/issues/2013))
- [codegen] Trim egraph per-function setup and scope upkeep ([#2015](https://github.com/paradigmxyz/solar/issues/2015))
- [codegen] Count outline runs before grouping ([#2016](https://github.com/paradigmxyz/solar/issues/2016))
- [codegen] Propagate edge equalities ([#1997](https://github.com/paradigmxyz/solar/issues/1997))
- [codegen] Price creation code at deployment gas ([#1998](https://github.com/paradigmxyz/solar/issues/1998))
- [evm] Duplicate straight-line loop joins ([#1962](https://github.com/paradigmxyz/solar/issues/1962))
- [evm] Duplicate small non-terminal joins ([#1940](https://github.com/paradigmxyz/solar/issues/1940))
- [mir] Narrow word stores whose tail is overwritten ([#1984](https://github.com/paradigmxyz/solar/issues/1984))
- [codegen] Log static two-word event data from scratch ([#1992](https://github.com/paradigmxyz/solar/issues/1992))
- [codegen] Return wide static ABI results from address 0 ([#1994](https://github.com/paradigmxyz/solar/issues/1994))
- [codegen] Return static ABI results from scratch ([#1982](https://github.com/paradigmxyz/solar/issues/1982))
- [stackify] Compute deeper operands after the pushes below them ([#1986](https://github.com/paradigmxyz/solar/issues/1986))
- [mir] Drop dead low masks before signextend ([#1987](https://github.com/paradigmxyz/solar/issues/1987))
- [mir] Shorten partial memory copies ([#1947](https://github.com/paradigmxyz/solar/issues/1947))
- [mir] If-convert boolean selects into and/or ([#1976](https://github.com/paradigmxyz/solar/issues/1976))
- [codegen] Copy packed arrays by word ([#1907](https://github.com/paradigmxyz/solar/issues/1907))
- [abi] Reserve encodings without runtime rounding ([#1967](https://github.com/paradigmxyz/solar/issues/1967))
- Mark bounded packed copies disjoint ([#1959](https://github.com/paradigmxyz/solar/issues/1959))
- [mir] Inline forwarders that widen arguments ([#1972](https://github.com/paradigmxyz/solar/issues/1972))
- [mir] Evaluate additive loop recurrences ([#1939](https://github.com/paradigmxyz/solar/issues/1939))
- Preserve known packed byte lengths ([#1956](https://github.com/paradigmxyz/solar/issues/1956))
- Fold ABI literals with preserved allocations ([#1952](https://github.com/paradigmxyz/solar/issues/1952))
- [mir] Unify inlining and share storage reads ([#1953](https://github.com/paradigmxyz/solar/issues/1953))
- [evm] Set the constructor heap only where read ([#1969](https://github.com/paradigmxyz/solar/issues/1969))
- [mir] Forward storage facts into sole successors ([#1968](https://github.com/paradigmxyz/solar/issues/1968))
- [abi] Encode returned aggregates without a cleaned copy ([#1966](https://github.com/paradigmxyz/solar/issues/1966))
- [mir] Keep checked booleans as words ([#1964](https://github.com/paradigmxyz/solar/issues/1964))
- [evm-ir] Derive EVM IR pass rules from the op table ([#1945](https://github.com/paradigmxyz/solar/issues/1945))
- [mir] Clean up memory and storage passes ([#1943](https://github.com/paradigmxyz/solar/issues/1943))
- [evm] Retain costly constants on the stack ([#1954](https://github.com/paradigmxyz/solar/issues/1954))
- Expand repeated data before Cancun ([#1946](https://github.com/paradigmxyz/solar/issues/1946))
- [mir] Remove overwritten memory fills ([#1961](https://github.com/paradigmxyz/solar/issues/1961))
- [abi] Bound word-array allocation lengths ([#1941](https://github.com/paradigmxyz/solar/issues/1941))
- [mir] Review loop and check-elim passes ([#1937](https://github.com/paradigmxyz/solar/issues/1937))
- [mir] Bound EVM data and code lengths ([#1948](https://github.com/paradigmxyz/solar/issues/1948))
- [mir] Blend complementary masks with xor ([#1950](https://github.com/paradigmxyz/solar/issues/1950))
- [mir] Skip short-copy direction checks ([#1955](https://github.com/paradigmxyz/solar/issues/1955))
- [mir] Clean up scalar, CFG, and call passes ([#1949](https://github.com/paradigmxyz/solar/issues/1949))
- [mir] Use byte stores for one-byte copy tails ([#1957](https://github.com/paradigmxyz/solar/issues/1957))
- [evm] Reuse zeros before PUSH0 ([#1932](https://github.com/paradigmxyz/solar/issues/1932))
- [evm] Clean up inlined return tails ([#1958](https://github.com/paradigmxyz/solar/issues/1958))
- [mir] Remove redundant immutable masks ([#1935](https://github.com/paradigmxyz/solar/issues/1935))
- [mir] Trust unmodified fallback calldata ([#1931](https://github.com/paradigmxyz/solar/issues/1931))
- [mir] Test high-bit range checks with shifts ([#1934](https://github.com/paradigmxyz/solar/issues/1934))
- [codegen] Isolate artifact data pools ([#1933](https://github.com/paradigmxyz/solar/issues/1933))
- [mir] Refresh the lowering passes ([#1944](https://github.com/paradigmxyz/solar/issues/1944))
- [mir] Reuse the pointer read for encodings ([#1929](https://github.com/paradigmxyz/solar/issues/1929))
- [mir] Refresh inliner classification and cleanup ([#1930](https://github.com/paradigmxyz/solar/issues/1930))
- [abi] Decode calldata slices in place ([#1923](https://github.com/paradigmxyz/solar/issues/1923))
- [mir] Trust decoded calldata bytes when copying ([#1928](https://github.com/paradigmxyz/solar/issues/1928))
- [mir] Reuse calls to pure leaf functions ([#1925](https://github.com/paradigmxyz/solar/issues/1925))
- [mir] Inline msg.data and struct-returning leaves ([#1924](https://github.com/paradigmxyz/solar/issues/1924))
- [mir] Drop stores into write-only allocations ([#1922](https://github.com/paradigmxyz/solar/issues/1922))
- [mir] Forward array lengths into loop bodies ([#1916](https://github.com/paradigmxyz/solar/issues/1916))
- [mir] Merge word tests into shared aborts ([#1919](https://github.com/paradigmxyz/solar/issues/1919))
- [codegen] Drop null memory pointer guards ([#1917](https://github.com/paradigmxyz/solar/issues/1917))
- [abi] Stage terminal payloads in scratch ([#1903](https://github.com/paradigmxyz/solar/issues/1903))
- [mir] Simplify calls, words and slot hashes ([#1902](https://github.com/paradigmxyz/solar/issues/1902))
- [sema] Pick query caches by key type ([#1857](https://github.com/paradigmxyz/solar/issues/1857))
- [codegen] Store unfolded constant fields directly ([#1813](https://github.com/paradigmxyz/solar/issues/1813))
- [codegen] Replace the frame-backed emitter with stackify ([#1730](https://github.com/paradigmxyz/solar/issues/1730))
- [codegen] Keep stack across buried branches ([#1756](https://github.com/paradigmxyz/solar/issues/1756))
- [codegen] Decode static returns from a slice ([#1751](https://github.com/paradigmxyz/solar/issues/1751))
- [codegen] Plan library and try call returns ([#1748](https://github.com/paradigmxyz/solar/issues/1748))
- [codegen] Decode single static returns in place ([#1746](https://github.com/paradigmxyz/solar/issues/1746))
- Use lean opts to optimise GAS ([#1660](https://github.com/paradigmxyz/solar/issues/1660))
- [parser] Shrink parser stack frames ([#1741](https://github.com/paradigmxyz/solar/issues/1741))
- [mir] Unroll counted loops ([#1652](https://github.com/paradigmxyz/solar/issues/1652))
- Retain repeated internal call arguments ([#1698](https://github.com/paradigmxyz/solar/issues/1698))
- Forward single-result internal tail calls ([#1697](https://github.com/paradigmxyz/solar/issues/1697))
- [codegen] Cache failed heap-prefix searches ([#1692](https://github.com/paradigmxyz/solar/issues/1692))
- [codegen] Expand scalar ISLE rules ([#1528](https://github.com/paradigmxyz/solar/issues/1528))
- [codegen] Shrink lowered MIR ([#1636](https://github.com/paradigmxyz/solar/issues/1636))
- [codegen] Record debug info as location runs ([#1632](https://github.com/paradigmxyz/solar/issues/1632))
- [codegen] Defer embedded contract bytecode ([#1615](https://github.com/paradigmxyz/solar/issues/1615))
- [cli] Skip dropping the compiler context ([#1613](https://github.com/paradigmxyz/solar/issues/1613))
- [codegen] Cut repeated work in planning, summaries and passes ([#1609](https://github.com/paradigmxyz/solar/issues/1609))
- [cli] Compute contract metadata during codegen ([#1605](https://github.com/paradigmxyz/solar/issues/1605))
- [codegen] Speed up analyses and spill planning ([#1606](https://github.com/paradigmxyz/solar/issues/1606))
- Reduce repeated EVM analysis ([#1592](https://github.com/paradigmxyz/solar/issues/1592))
- [cli] Parallelize standard JSON outputs ([#1602](https://github.com/paradigmxyz/solar/issues/1602))
- Reuse EVM work during size rescue ([#1585](https://github.com/paradigmxyz/solar/issues/1585))
- Prioritize ready contracts during codegen ([#1562](https://github.com/paradigmxyz/solar/issues/1562))
- [codegen] Prune stack resynthesis removal orders ([#1581](https://github.com/paradigmxyz/solar/issues/1581))
- [codegen] Speed up MIR and EVM IR passes ([#1556](https://github.com/paradigmxyz/solar/issues/1556))
- [lsp] Measure pending request latency ([#1546](https://github.com/paradigmxyz/solar/issues/1546))
- [codegen] Avoid repeated analysis work ([#1503](https://github.com/paradigmxyz/solar/issues/1503))
- [codegen] Improve scalar optimizations ([#1523](https://github.com/paradigmxyz/solar/issues/1523))
- [mir] Inline checked helpers in hot loops ([#1516](https://github.com/paradigmxyz/solar/issues/1516))
- [mir] Simplify select and base-two EXP ([#1493](https://github.com/paradigmxyz/solar/issues/1493))
- [evm] Refine peepholes and terminal layout ([#1489](https://github.com/paradigmxyz/solar/issues/1489))
- [lsp] Reuse identical workspace analysis ([#1485](https://github.com/paradigmxyz/solar/issues/1485))
- [lsp] Filter quick-fix diagnostics early ([#1488](https://github.com/paradigmxyz/solar/issues/1488))
- [lsp] Cache URI path conversions ([#1492](https://github.com/paradigmxyz/solar/issues/1492))
- [lsp] Reduce completion and location overhead ([#1473](https://github.com/paradigmxyz/solar/issues/1473))
- [lsp] Avoid scanning symbols for rename ([#1483](https://github.com/paradigmxyz/solar/issues/1483))
- [lsp] Reuse rope indexes for document edits ([#1471](https://github.com/paradigmxyz/solar/issues/1471))
- [lsp] Speed up call hierarchy expansion ([#1482](https://github.com/paradigmxyz/solar/issues/1482))
- [lsp] Reduce signature-help request overhead ([#1474](https://github.com/paradigmxyz/solar/issues/1474))
- [lsp] Speed up selection and type hierarchy ([#1478](https://github.com/paradigmxyz/solar/issues/1478))
- [lsp] Reuse signature statement boundaries ([#1470](https://github.com/paradigmxyz/solar/issues/1470))
- [lsp] Reduce rename request overhead ([#1466](https://github.com/paradigmxyz/solar/issues/1466))
- [lsp] Reuse parsing across quick fixes ([#1465](https://github.com/paradigmxyz/solar/issues/1465))
- [lsp] Reduce repeated analysis work ([#1461](https://github.com/paradigmxyz/solar/issues/1461))
- [lsp] Batch descending document edits ([#1459](https://github.com/paradigmxyz/solar/issues/1459))
- [lsp] Prioritize interactive analysis ([#1460](https://github.com/paradigmxyz/solar/issues/1460))
- [lsp] Reuse rename position indexes ([#1458](https://github.com/paradigmxyz/solar/issues/1458))
- [lsp] Defer workspace search indexing ([#1457](https://github.com/paradigmxyz/solar/issues/1457))
- [lsp] Narrow lexical scope lookups ([#1452](https://github.com/paradigmxyz/solar/issues/1452))
- [lsp] Reuse validated aggregate indexes ([#1450](https://github.com/paradigmxyz/solar/issues/1450))
- [lsp] Skip import lexing outside strings ([#1451](https://github.com/paradigmxyz/solar/issues/1451))
- [lsp] Accelerate repeated query context ([#1453](https://github.com/paradigmxyz/solar/issues/1453))
- [lsp] Reduce folding-range latency ([#1454](https://github.com/paradigmxyz/solar/issues/1454))
- [lsp] Index repeated query work ([#1442](https://github.com/paradigmxyz/solar/issues/1442))
- [lsp] Index point queries ([#1447](https://github.com/paradigmxyz/solar/issues/1447))
- [lsp] Cache diagnostic fingerprints ([#1445](https://github.com/paradigmxyz/solar/issues/1445))
- [lsp] Cache CodeLens reference counts ([#1443](https://github.com/paradigmxyz/solar/issues/1443))
- [lsp] Speed up member and position queries ([#1440](https://github.com/paradigmxyz/solar/issues/1440))
- [lsp] Index document symbol children ([#1441](https://github.com/paradigmxyz/solar/issues/1441))
- [lsp] Cut update and completion overhead ([#1437](https://github.com/paradigmxyz/solar/issues/1437))
- [lsp] Speed up workspace and symbol queries ([#1435](https://github.com/paradigmxyz/solar/issues/1435))
- [lsp] Cache workspace path index ([#1436](https://github.com/paradigmxyz/solar/issues/1436))
- [lsp] Reduce signature-help prefix lexing
- [lsp] Speed up signature help ([#1432](https://github.com/paradigmxyz/solar/issues/1432))
- [lsp] Cache completion positions ([#1431](https://github.com/paradigmxyz/solar/issues/1431))
- [lsp] Defer import path parsing ([#1425](https://github.com/paradigmxyz/solar/issues/1425))
- [lsp] Skip import scans for code completion ([#1426](https://github.com/paradigmxyz/solar/issues/1426))
- [lsp] Cache workspace import index ([#1422](https://github.com/paradigmxyz/solar/issues/1422))
- [lsp] Reuse unchanged analysis batches ([#1421](https://github.com/paradigmxyz/solar/issues/1421))
- [lsp] Skip import scans for indexed definitions ([#1416](https://github.com/paradigmxyz/solar/issues/1416))
- [lsp] Reuse signature-help position index ([#1411](https://github.com/paradigmxyz/solar/issues/1411))
- [codegen] Cut repeated backend and MIR analyses ([#1358](https://github.com/paradigmxyz/solar/issues/1358))
- [lsp] Index cached selection ranges ([#1406](https://github.com/paradigmxyz/solar/issues/1406))
- [lsp] Reduce query work and hierarchy memory ([#1396](https://github.com/paradigmxyz/solar/issues/1396))
- [lsp] Reduce query work and index memory ([#1395](https://github.com/paradigmxyz/solar/issues/1395))
- [lsp] Reduce indexing time and memory ([#1393](https://github.com/paradigmxyz/solar/issues/1393))
- [lsp] Optimize folding range ([#1391](https://github.com/paradigmxyz/solar/issues/1391))
- [lsp] Reuse unchanged analysis snapshots ([#1389](https://github.com/paradigmxyz/solar/issues/1389))
- [lsp] Cache source file URLs ([#1387](https://github.com/paradigmxyz/solar/issues/1387))
- Skip IR validation after unchanged passes ([#1386](https://github.com/paradigmxyz/solar/issues/1386))
- [codegen] Defer validator cycle checks ([#1380](https://github.com/paradigmxyz/solar/issues/1380))
- [lsp] Cache derived document data ([#1351](https://github.com/paradigmxyz/solar/issues/1351))
- [codegen] Index, push, and pop storage bytes in place ([#1366](https://github.com/paradigmxyz/solar/issues/1366))
- [codegen] Skip redundant validation and layout ([#1379](https://github.com/paradigmxyz/solar/issues/1379))
- [lsp] Speed up benchmark reporting ([#1362](https://github.com/paradigmxyz/solar/issues/1362))
- [lsp] Optimize span location conversion ([#1353](https://github.com/paradigmxyz/solar/issues/1353))
- [lsp] Cache workspace path matches ([#1350](https://github.com/paradigmxyz/solar/issues/1350))
- [codegen] Improve EVM stack scheduling ([#1305](https://github.com/paradigmxyz/solar/issues/1305))
- [codegen] Rematerialize nullary reads ([#1286](https://github.com/paradigmxyz/solar/issues/1286))
- Reduce release binary size ([#1233](https://github.com/paradigmxyz/solar/issues/1233))
- [codegen] Use stack-native static ABI ([#1162](https://github.com/paradigmxyz/solar/issues/1162))
- [codegen] Compact interprocedural codegen ([#1156](https://github.com/paradigmxyz/solar/issues/1156))
- [codegen] Lazily materialize static args ([#1154](https://github.com/paradigmxyz/solar/issues/1154))
- [codegen] Materialize constant data ([#1153](https://github.com/paradigmxyz/solar/issues/1153))
- [codegen] Preserve caller stack ([#1150](https://github.com/paradigmxyz/solar/issues/1150))
- [codegen] Compact static frames ([#1144](https://github.com/paradigmxyz/solar/issues/1144))
- [codegen] Inline tiny leaf helpers ([#1143](https://github.com/paradigmxyz/solar/issues/1143))
- [codegen] Return static calls on stack ([#1141](https://github.com/paradigmxyz/solar/issues/1141))
- [codegen] Improve switch lowering ([#1002](https://github.com/paradigmxyz/solar/issues/1002))
- [codegen] Optimize after MIR lowering ([#1112](https://github.com/paradigmxyz/solar/issues/1112))
- [codegen] Elide memory self-stores ([#1097](https://github.com/paradigmxyz/solar/issues/1097))
- [codegen] Order MIR for EVM stack ([#1045](https://github.com/paradigmxyz/solar/issues/1045))
- [codegen] Improve stack scheduling ([#1005](https://github.com/paradigmxyz/solar/issues/1005))
- [codegen] Index instruction results ([#1040](https://github.com/paradigmxyz/solar/issues/1040))
- [codegen] Keep join phis on stack ([#1032](https://github.com/paradigmxyz/solar/issues/1032))
- [codegen] Improve pass ordering ([#1017](https://github.com/paradigmxyz/solar/issues/1017))
- [codegen] Improve EVM IR peepholes ([#994](https://github.com/paradigmxyz/solar/issues/994))
- [codegen] Optimize EVM block layout ([#966](https://github.com/paradigmxyz/solar/issues/966))
- [codegen] Speed up MIR validation ([#963](https://github.com/paradigmxyz/solar/issues/963))
- [codegen] Use dense sets for IDs ([#958](https://github.com/paradigmxyz/solar/issues/958))

[#921]: https://github.com/paradigmxyz/solar/issues/921
[#930]: https://github.com/paradigmxyz/solar/issues/930
[#933]: https://github.com/paradigmxyz/solar/issues/933
[#939]: https://github.com/paradigmxyz/solar/issues/939
[#941]: https://github.com/paradigmxyz/solar/issues/941
[#942]: https://github.com/paradigmxyz/solar/issues/942
[#948]: https://github.com/paradigmxyz/solar/issues/948
[#950]: https://github.com/paradigmxyz/solar/issues/950
[#952]: https://github.com/paradigmxyz/solar/issues/952
[#954]: https://github.com/paradigmxyz/solar/issues/954
[#955]: https://github.com/paradigmxyz/solar/issues/955
[#980]: https://github.com/paradigmxyz/solar/issues/980
[#1004]: https://github.com/paradigmxyz/solar/issues/1004
[#1025]: https://github.com/paradigmxyz/solar/issues/1025
[#1026]: https://github.com/paradigmxyz/solar/issues/1026
[#1031]: https://github.com/paradigmxyz/solar/issues/1031
[#1035]: https://github.com/paradigmxyz/solar/issues/1035
[#1050]: https://github.com/paradigmxyz/solar/issues/1050
[#1080]: https://github.com/paradigmxyz/solar/issues/1080
[#1102]: https://github.com/paradigmxyz/solar/issues/1102
[#1111]: https://github.com/paradigmxyz/solar/issues/1111
[#1130]: https://github.com/paradigmxyz/solar/issues/1130
[#1187]: https://github.com/paradigmxyz/solar/issues/1187
[#1209]: https://github.com/paradigmxyz/solar/issues/1209
[#1213]: https://github.com/paradigmxyz/solar/issues/1213
[#1243]: https://github.com/paradigmxyz/solar/issues/1243
[#1288]: https://github.com/paradigmxyz/solar/issues/1288
[#1320]: https://github.com/paradigmxyz/solar/issues/1320
[#1325]: https://github.com/paradigmxyz/solar/issues/1325
[#1327]: https://github.com/paradigmxyz/solar/issues/1327
[#1344]: https://github.com/paradigmxyz/solar/issues/1344
[#1368]: https://github.com/paradigmxyz/solar/issues/1368
[#1373]: https://github.com/paradigmxyz/solar/issues/1373
[#1382]: https://github.com/paradigmxyz/solar/issues/1382
[#1408]: https://github.com/paradigmxyz/solar/issues/1408
[#1415]: https://github.com/paradigmxyz/solar/issues/1415
[#1422]: https://github.com/paradigmxyz/solar/issues/1422
[#1434]: https://github.com/paradigmxyz/solar/issues/1434
[#1447]: https://github.com/paradigmxyz/solar/issues/1447
[#1471]: https://github.com/paradigmxyz/solar/issues/1471
[#1498]: https://github.com/paradigmxyz/solar/issues/1498
[#1509]: https://github.com/paradigmxyz/solar/issues/1509
[#1525]: https://github.com/paradigmxyz/solar/issues/1525
[#1574]: https://github.com/paradigmxyz/solar/issues/1574
[#1602]: https://github.com/paradigmxyz/solar/issues/1602
[#1646]: https://github.com/paradigmxyz/solar/issues/1646
[#1737]: https://github.com/paradigmxyz/solar/issues/1737
[#1740]: https://github.com/paradigmxyz/solar/issues/1740
[#1741]: https://github.com/paradigmxyz/solar/issues/1741
[#1752]: https://github.com/paradigmxyz/solar/issues/1752
[#1759]: https://github.com/paradigmxyz/solar/issues/1759
[#1782]: https://github.com/paradigmxyz/solar/issues/1782
[#1810]: https://github.com/paradigmxyz/solar/issues/1810
[#1821]: https://github.com/paradigmxyz/solar/issues/1821
[#1823]: https://github.com/paradigmxyz/solar/issues/1823
[#1857]: https://github.com/paradigmxyz/solar/issues/1857
[#1878]: https://github.com/paradigmxyz/solar/issues/1878
[#1896]: https://github.com/paradigmxyz/solar/issues/1896
[#2006]: https://github.com/paradigmxyz/solar/issues/2006
[#2021]: https://github.com/paradigmxyz/solar/issues/2021
[#2023]: https://github.com/paradigmxyz/solar/issues/2023
[#2045]: https://github.com/paradigmxyz/solar/issues/2045
[#2046]: https://github.com/paradigmxyz/solar/issues/2046
[#2047]: https://github.com/paradigmxyz/solar/issues/2047
[#2061]: https://github.com/paradigmxyz/solar/issues/2061
[#2093]: https://github.com/paradigmxyz/solar/issues/2093
[#2101]: https://github.com/paradigmxyz/solar/issues/2101
[#2109]: https://github.com/paradigmxyz/solar/issues/2109
[#2129]: https://github.com/paradigmxyz/solar/issues/2129
[#2133]: https://github.com/paradigmxyz/solar/issues/2133
[#2141]: https://github.com/paradigmxyz/solar/issues/2141
[#2142]: https://github.com/paradigmxyz/solar/issues/2142
[#2143]: https://github.com/paradigmxyz/solar/issues/2143
[#2149]: https://github.com/paradigmxyz/solar/issues/2149
[#2202]: https://github.com/paradigmxyz/solar/issues/2202
[`2c31d78`]: https://github.com/paradigmxyz/solar/commit/2c31d78c0d

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
