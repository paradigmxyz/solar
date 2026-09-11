# Verified EVM word rewrites

This is an offline search and SMT verification lane for the **actual ISLE source
compiled into the optimizer**. It currently gates `word.isle`, `word_sequence.isle` and
`stack_select.isle`, plus the physical rules in `stack_peephole.isle` and
`late_word.isle`. CI also explicitly verifies and replays `egraph.isle` with
the larger budgets described below. The compiler itself has no solver dependency.

```sh
uv run scripts/evm-rules/test.py
uv run scripts/evm-rules/verify.py verify \
  --output target/evm-rules/proofs.json --artifacts target/evm-rules/smt
```

The script pins Z3 through its inline dependency metadata. Each result records
the source and rule hashes, solver version, instruction-selection and extractor
hashes, verifier implementation hash, trusted Rust source hashes, and any
trusted extractor contracts. The optional artifacts are standalone SMT-LIB counterexample queries. An equivalent
rule produces `unsat`. Difficult shift queries can be split into
257 exhaustive cases: each count from 0 through 255 and the entire saturating
range. The checker also verifies partition coverage. Every case must finish
with UNSAT within the partition budget; partial coverage never proves a rule.
`SIGNEXTEND` indices use 32 exhaustive cases: 0 through 30, then the entire
identity range from 31 upward. If one index also serves as a shift count, the
partition uses the larger boundary. The final case always retains a symbolic
index, so it includes arbitrarily large values rather than one representative.
Counts introduced only by guards are included, such as the exponent that
constrains a divisor to a power of two. With several indices, the checker
partitions the smallest domain, breaking ties by name, and leaves every other
input symbolic in every case. It does not assume that distinct indices are
equal or restrict them to sampled values. These cases can still time out;
partitioning more source shapes does not make the legacy audit complete.
The report lists every saved query; replay one with `z3 path/to/rule.smt2` or
`cvc5 --lang smt2 path/to/rule.smt2`.

For complete replay with cvc5, export exhaustive index partitions even when Z3
can prove the original query directly:

```sh
uv run scripts/evm-rules/verify.py verify --partition-shifts \
  --output target/evm-rules/proofs.json --artifacts target/evm-rules/smt
uv run scripts/evm-rules/replay.py target/evm-rules/proofs.json \
  --solver cvc5 --output target/evm-rules/cvc5.json
```

Install cvc5 separately and run both commands from the repository root; saved
query paths are relative to the verifier's working directory. Queries declare
`QF_BV` (or `QF_ABV` for balance snapshots) and rename free constants to portable SMT-LIB identifiers, retaining
their original names in comments. The report fingerprints every query with
SHA-256. Replay checks the exact manifest and bytes, including partition
coverage and every physical-stack variant. Missing or changed queries fail.

Replay requires UNSAT from every query. It first uses cvc5's internal
bitblaster (`--bv-solver=bitblast-internal`), which handles the nested-shift
output-bit obligations more reliably than the default backend. It retries
only timeouts or unknown results with the default bitvector strategy, then
`--solve-bv-as-int=sum`. The default limit is five seconds per strategy and four
concurrent queries, configurable with `--timeout-ms` and `--jobs`. All attempts
and the solver version are recorded. SAT, exhausted time limits, parse errors,
and process failures exit nonzero; SAT is a solver disagreement until separately
replayed in the concrete model. This checks the exported formulas with another
solver, not the semantics that generated them or an independent proof
certificate. The proof job runs on native Linux ARM64, downloads the matching
cvc5 1.2.0 release with a pinned SHA-256, exports exhaustive partitions, and
requires both Z3 verification and complete cvc5 replay to pass.

The CLZ model selects the half containing the highest set bit in eight steps
and constructs a nine-bit count before extending it to an EVM word. Zero
explicitly produces 256. This avoids the previous 256-deep conditional chain
without assuming anything about the input. A regression proves this encoding
equal to the full bit scan for every 256-bit word, and cvc5 integration tests
replay the actual CLZ rules. Neither the compiler's rules nor the proof
obligations' guards change.

When a rule contains several shift or SIGNEXTEND indices, index partitioning
refines only the remaining tail with the next index. For two SIGNEXTEND
indices, this covers 31 concrete values of the first, then 31 concrete values
of the second while the first is at least 31, then both indices at least 31.
All other inputs remain symbolic, including both indices in the final case.
The 63 cases replace one difficult symbolic tail without a Cartesian product;
the exported coverage query must still prove that every input is covered.
`--index-partition-timeout-ms` sets a shared budget for building and proving
all cases of one rule; zero uses `--timeout-ms`. CI gives the e-graph index
stage 30 seconds so a five-second total budget does not prematurely discard
these smaller queries. Replay still has a
five-second limit per strategy per query and fails on every exhausted query.

Word verification has an optional, explicit cvc5 fallback:

```sh
uv run scripts/evm-rules/verify.py verify crates/codegen/isle/egraph.isle \
  --fallback-solver cvc5 --output target/evm-rules/legacy.json \
  --artifacts target/evm-rules/legacy-smt
```

It runs only after Z3 established satisfiable preconditions and left equality
incomplete. It checks the **complete original query**, including specialization
obligations. Only UNSAT replaces an incomplete result; no proved prefix of a
partition is accepted. A successful whole-query proof replaces partial partition
artifacts even with `--partition-shifts`, because cvc5 can replay that complete
query directly. Failed attempts preserve the original query alongside any
partial partitions. The rule records the executable, version, query hash,
strategies and process results; the top-level `solver` remains the primary Z3
version and `fallback_solver` identifies the optional backend.

SAT from the fallback remains an unproved failure with an explicit reason until
it has an independent concrete replay; it is never reported as a proved rule or
a replay-confirmed counterexample. Neither an inapplicable rule nor a Z3
counterexample is retried. Missing executables, parse errors and exhausted
limits fail closed. The legacy audit can have incomplete shift proofs, so this command can exit
nonzero. It does not waive them or add them to the default five-file CI gate.
CI also exercises selected legacy division, remainder and comparison rules
from the actual source in regression tests using the installed cvc5; local runs skip only that optional integration
test when cvc5 is absent. The five-file gate still requires Z3 followed by cvc5
replay for all its rules; the additional e-graph gate uses the explicit fallback
and bit budget and also requires complete cvc5 replay.

For word queries that remain incomplete, opt into an additional budget for
proving every output bit separately:

```sh
uv run scripts/evm-rules/verify.py verify crates/codegen/isle/egraph.isle \
  --fallback-solver cvc5 --bit-partition-timeout-ms 120000 \
  --output target/evm-rules/legacy-bits.json \
  --artifacts target/evm-rules/legacy-bits-smt
```

All input words remain fully symbolic in every bit query, including independent
256-bit shift counts and their saturating ranges. Each query retains the
original guards and any constant-specialization obligation. Word equality is
the conjunction of equality at all 256 output bits, so no input sampling or
extra range assumptions are involved. The budget is shared across all bits of
one rule, rather than renewed per bit. All 256 queries must finish with UNSAT;
a timeout leaves the rule unproved and preserves the original whole-word query
alongside the attempted bit queries. SAT witnesses replay as complete words in
the independent integer evaluator. Replay rejects a proof missing any bit or
claiming a shorter word width. A separate coverage query is unnecessary here:
the fixed list of output positions is exactly 0 through 255.

This option runs only after satisfiable applicability and incomplete earlier
proof attempts. It cannot override an inapplicable rule, a counterexample, or
cvc5's SAT or process-error result. Failed cvc5 timeout attempts remain recorded
when bit proofs subsequently succeed. The default gate and solver time limits
are unchanged. The e-graph audit is a separate required step in the same CI job,
which has a twenty-minute wall-time limit for both proof and replay lanes.

Only UNSAT establishes equivalence. SAT must replay as different outputs in a
separate Python integer evaluator. Timeouts, unsupported terms and unsatisfiable
preconditions are distinct failures, never proofs. Verification exits nonzero
unless every selected rule is proved. Empty rule files fail too. CI runs the
checker's regression tests and verifies every rule in all five gated files.
Failures also print the source file, rule line, status and reason in the job log.
Applicability and equality use separate solver queries so the satisfiability
check does not disable Z3's one-shot bitvector preprocessing. Both queries retain
all rule preconditions, and unsatisfiable preconditions still fail verification.
CI uploads the report and SMT queries only when this job fails, retaining them
for seven days to diagnose failures and replay the exact solver queries.

## Semantics and trusted boundary

Three semantic memory-object address projections are modeled under the selected
`EvmMemoryLayout` policy: payload, direct struct field, and array element
addresses. The reader checks their field names and types against the generated
ISLE prelude, whose hash is recorded. Object operands denote their leading
pointer words. Payload projection distinguishes a slice's existing payload
pointer from an object reference preceding its header. The model includes all
four object kinds, full u32 element strides, full u64 field counts and indices,
the policy's saturating field-offset multiplication, and full-width wrapping
address arithmetic. Layout length does not affect element-address calculation.
Valid field ranges and applicable layout kinds are structural preconditions,
not assumed equalities between the rewrite's two sides. Removing the actual
source guards yields independently replayed counterexamples in regression tests.

These proofs establish address-word equality only. They do not establish memory
contents, allocation safety, bounds checks, aliasing, address-space selection,
or correctness of typed slice erasure. The Rust layout policy, type definitions,
memory-object lowering and slice lowering are explicitly fingerprinted trusted
sources. MIR regression tests compare projection lowering with and without the
e-graph pass for memory and calldata slices.

`ADDRESS`, `BALANCE`, and `SELFBALANCE` use one executing-account address and
one explicit account-balance snapshot. The address is a 160-bit symbolic input;
balances are an unconstrained array from 160-bit keys to 256-bit words.
`BALANCE` selects the low 160 bits of its operand, and `SELFBALANCE` selects
the executing account. `ADDRESS` zero-extends the address to one word. This
matches the operation definitions in [EIP-1884](https://eips.ethereum.org/EIPS/eip-1884)
and the [execution client's BALANCE implementation](https://github.com/ethereum/go-ethereum/blob/master/core/vm/instructions.go).
The reader still checks opcode selection and records the trusted
`current_address` extractor and fork-availability contracts.

Balance queries use quantifier-free bitvectors and arrays (`QF_ABV`); pure-word
queries retain `QF_BV`. Export accepts only balance arrays with a 160-bit domain
and 256-bit range, and still rejects arbitrary uninterpreted functions. On SAT,
the checker records the current address and every observed account balance,
then independently replays the complete expressions with integer addresses
and a concrete dictionary, including nested balance reads. Tests reject an
incorrect replacement for another account and check upper-bit truncation.

This proves returned-word equality within one state snapshot. It does not
model calls, balance mutations, gas, access-list warming, out-of-gas behavior,
or opcode availability on a particular fork. It must not be used to justify
balance CSE across calls or code motion across state changes. Unmodeled state
operations continue to fail, rather than becoming unconstrained functions.
The ISLE reader permits balance reads only at the instruction roots being
replaced. It rejects nested balance producers, which could have executed before
an intervening call; a shared-state assumption is not silently added for them.

The compiled balance-mask rules remove `address & mask` before `BALANCE` when
the mask preserves all low 160 bits. Their single-use and same-block guards
restrict profitability; the proof checks returned-word equality for arbitrary
addresses and masks satisfying the bit condition. Removing that condition in
the regression tests produces independently replayed counterexamples selecting
different accounts. The compiler keeps the account read at its original
instruction and the runtime tests cover dirty upper bits and funded accounts.

The model uses 256-bit bitvectors, wrapping arithmetic, full-width saturating
shift counts, unsigned comparisons and two's-complement signed comparisons.
SIGNEXTEND selects one of 31 constant signed byte widths, retaining the input
for every index at or above 31. This avoids nested variable shifts in SMT. A
regression proves equality with the shift-based definition over all words and
all indices using 32 exhaustive cases and a separate coverage obligation.
DIV, SDIV, MOD and SMOD return zero for a zero divisor. SDIV rounds toward zero
and wraps the minimum signed word divided by minus one; SMOD takes the dividend's
sign. ADDMOD and MULMOD use a 512-bit intermediate. BYTE and SIGNEXTEND check the
full index before shifting. EXP uses square-and-multiply for literal exponents,
or all 256 exponent bits when the base is literal. When both are symbolic,
literal equalities from the guards can specialize the word model. The exported
query requires both the substitution and the resulting equality to hold:
`guards && (!substitution_equalities || specialized_lhs != specialized_rhs)`
must be UNSAT. A substitution not implied by the guards fails verification,
including in partitioned queries. No input is chosen from a satisfying model
to make an unsupported operation appear proved. Unconstrained EXP with both
operands symbolic remains outside the current model.
The tests cross-check the symbolic and independent concrete models at these
boundaries; `tests/ui/codegen/mir/egraph/word_rules_runtime.sol` also checks actual
compiled execution, including cases where ordinary integer identities are wrong.

The reader expands the schema-generated `extractors.isle`, reads each rule and
its `if-let` guards, and checks scalar operation/operand-shape bindings against
`select.isle`. A changed or unmodeled opcode selection fails closed. We do not
maintain a second list of optimization identities for verification. Structural
inequality of ValueIds never implies inequality of their runtime words.

This is a proof of **word equality under the model and stated contracts**. The
solver, semantics, reader, generated bindings, Rust extractors/constructors and
backend implementation are trusted. Range predicates are conditional contracts,
not proofs of the Rust analyses that implement them. Resident-value selection
assumes the original expression has already executed and remains available.
The new `word.isle` rules need no range-analysis predicates.
The older `has_known_sign_bit` contract means bit 255 is set; its false result
does not imply the bit is clear. The Rust extractor is conservative and remains
part of the trusted, fingerprinted implementation.

Memory contents, storage, calls, exceptions, gas observability, stack bounds, code motion
and whole-program correctness are outside this proof. ISLE priorities affect
matching but not an individual rule's equality obligation. We overapproximate
structural/fork guards; actual opcode availability and profitability remain the
compiler's responsibility. There is no claim of a verified compiler or an
independently checked proof certificate.

The physical stack lane checks the six compiled rules in `stack_peephole.isle`
directly, including every supported DUP/SWAP depth from 1 through 235 and every
legal EXCHANGE pair. It compares every touched word, the final height, required
input depth and peak growth; an arbitrary deeper prefix stays unchanged.
Malformed input bytecode and out-of-gas behavior are excluded. The Rust window
facets, edits and target lowering remain trusted and are recorded by hash.
Other peepholes, especially memory and branch rewrites, are not covered by this
lane. Unknown syntax, guards, edits or operations fail verification.

The late word lane models bounded physical windows around shift-count
computations. One extractor requires a closed computation that leaves exactly
one word and reads no incoming stack items. The other tracks a unique shift base
through stack permutations: the body cannot duplicate, consume, or inspect that
word, and must leave it directly below the count. Other stack results remain
independent of the base. Neither body may observe gas/PC or transfer control;
its instructions and external reads stay in order. The proof checks the
surrounding stack expression and compares stack heights. Closed-body peak growth
decreases by one; the protected body's entry height and internal growth are
unchanged, and removing the decrement suffix cannot increase its peak. The Rust extractor,
target profitability guard and edit implementation remain trusted and hashed.
Missing guards and changed edits fail closed; changing the shift operation must
produce a replayed counterexample in the checker tests.

This lane implements `(1 << n) - 1 => ~(MAX << n)` only after outlining and
stack cleanup. Earlier MIR materialization shortened individual expressions but
lost sharing in a packed-storage fixture. The closed form removes one byte and
one gas on PUSH0 targets. The protected-base form also removes a trailing
`PUSH1 1; SWAP1`, saving three bytes and four gas with PUSH0, or two bytes and
three gas on earlier shift-capable targets. The closed form stays unchanged on
pre-PUSH0 targets because neither target cost improves. Earlier sharing
decisions remain unchanged in both cases.

An audit of the older rules is available explicitly:

```sh
uv run scripts/evm-rules/verify.py verify crates/codegen/isle/egraph.isle \
  --timeout-ms 1000 --output target/evm-rules/audit.json
```

This short-budget audit can time out on division/remainder and variable-index
queries. For a complete audit, use the explicit cvc5 fallback and output-bit
budget described above; use `--partition-shifts` to export exhaustive input
partitions for cross-solver replay. The three address projections and the local
balance rule now have explicit models with the trusted boundaries documented
above. Every selected rule must prove, and every exported query must replay;
do not ignore unknown or unsupported results. CI runs this file separately
from the default five-file selection and requires both lanes to pass.

## Discovering candidates

Mine candidates from actual MIR artifacts before searching:

```sh
uv run scripts/evm-rules/verify.py mine \
  target/codegen-bench/baseline/artifacts/*/solar/mir.mir \
  --max-ops 8 --max-seeds 128 \
  --output target/evm-rules/mined.json \
  --emit-seeds target/evm-rules/mined-seeds.json
uv run scripts/evm-rules/verify.py discover \
  --variables x y z --max-ops 2 --max-rhs-ops 2 \
  --seed-expressions target/evm-rules/mined-seeds.json \
  --output target/evm-rules/discovery.json \
  --emit-isle target/evm-rules/candidates.isle
```

The miner recognizes direct word operations within one straight-line region.
Block boundaries and unrecognized instructions end the region. Shared producers
become independent inputs instead of receiving deletion credit. Trees have at
most three inputs and sixteen operations. The report ranks static occurrences
times target tree cost and records source hashes, functions, blocks and lines;
this is a search priority, not a claim about runtime frequency or realized savings.
Empty mining results exit unsuccessfully. Mining neither proves nor installs rules.
Pass `--abstract-subtrees` to also replace an internal operation subtree or a
nontrivial literal with an independent input. All occurrences of the selected
subtree share that input, and alpha-renaming preserves the three-input bound.
The literals zero, one and MAX remain available for identities. Each candidate
abstracts at most one distinct subtree; the report identifies its abstracted
occurrences and source locations. This exposes general patterns hidden by deep
shift-count arithmetic or repeated literal masks. Equivalence must hold for
every value of the new input, not merely values observed in the source program.
The exact emitted ISLE must pass verification, then gas/size measurements decide
whether to integrate it. Mining the optimized runtime corpus motivated the
doubling rule in `word.isle` and odd-word recipes in `word_sequence.isle`.
The doubling rule keeps the producer at its original position: rebuilding it at
the later addition regressed stack traffic across intervening computations.

```sh
uv run scripts/evm-rules/verify.py discover \
  --ops and or xor not --variables x y m --max-ops 3 --max-rhs-ops 2 \
  --max-expressions 10000 --max-rules 256 \
  --evm-version osaka --objective gas \
  --output target/evm-rules/discovery.json \
  --emit-isle target/evm-rules/candidates.isle
```

The search enumerates bounded expression trees over up to three variables.
Boundary samples and deterministic random samples propose equalities. Every
representative substitution requires SMT proof; counterexamples refine the
sample buckets, and unknown results keep separate representatives. Cheaper
proved spellings replace expensive representatives so enumeration can build on
them. This is bounded enumerative search inspired by cvec-based discovery, not
a port of Ruler or unrestricted equality saturation.

Use `--seed-expressions` to search replacements for deeper input trees without
enumerating every tree of their size. For example, the checked-in seeds include
`(x | y) - (x & y)`. A one-operation frontier can discover `x ^ y` even though
the input has three operations:

```sh
uv run scripts/evm-rules/verify.py discover \
  --seed-expressions scripts/evm-rules/seeds.json \
  --ops and or xor not sub add --variables x y \
  --max-ops 2 --max-rhs-ops 2 --max-expressions 1000 --max-rules 64 \
  --output target/evm-rules/seeded.json \
  --emit-isle target/evm-rules/seeded.isle
```

The seed file is a JSON array of expression trees. An operation is an array
such as `["sub", ["or", "x", "y"], ["and", "x", "y"]`; leaves are declared
variable names or integers with exported Target prices. Files contain at most
128 trees of at most 16 operations each. Arity, fork availability and modeled
semantics are checked before searching. Seeds may use operations absent from
`--ops`, which controls the replacement frontier. With this option, only
replacements for supplied seeds are emitted. The report records the seed file
hash, input trees and how many obtained a proved cheaper replacement.

Seeds do not enlarge the enumeration budget or enter the frontier. They are
compared against the final representatives, including a partial frontier when
the expression budget is exhausted. Samples only select solver queries; every
seed replacement and its emitted ISLE must still be proved. Counterexample
refinement extends cached sample vectors before reusing bucket keys. A seed
with no proved cheaper match is left unresolved, including solver timeouts.
This is a bounded local search, not a guarantee of finding the cheapest program.

The default frontier uses variables. Zero, one and MAX remain possible results;
`--include-constants` also enumerates literal inputs. `--constants` selects a
specialized input domain from the exported Target table. Zero, one and MAX
always remain available as results, even when excluded from that input domain.
The table covers byte indexes, common shift counts, powers of two and field masks.
For example, search packed-byte patterns with:

```sh
uv run scripts/evm-rules/verify.py discover \
  --ops and shr byte --result-ops byte --variables x --include-constants \
  --constants 0 1 8 30 31 255 256 --max-ops 2 --max-rhs-ops 2 \
  --output target/evm-rules/packed.json \
  --emit-isle target/evm-rules/packed.isle
```

`--result-ops` focuses the emitted candidates on useful replacement
roots without changing the equivalence checks. Literal matching emits explicit
equality guards, including full-word constants assembled from four checked u64
limbs. Search bounds and expression budget exhaustion are recorded. The emitter expands commutative spellings and
alpha-renames them, then verifies the **emitted ISLE** again. Single-operation
or leaf replacements use `rewrite`. Larger replacements use `sequence_rewrite`,
with fallible `make` and `imm` constructors bound by `if-let` clauses. `--max-rhs-ops` bounds the replacement tree (default two). Place accepted
ordinary rules in `word.isle` and recipes in `word_sequence.isle`; a mixed
candidate file is a proposal, not an automatically registered compiler rule set.
Failed emitted-source checks retain the discovery report and cause a nonzero exit.
No candidates produces an empty candidate file and an explicit `no_candidates`
result. Discovery never changes the compiler's rule files automatically.

The initial 36 mixed-bitwise spellings cover nine families with two different
binary bitwise children. Six arithmetic spellings were proposed manually. A
second mixed arithmetic/bitwise search supplied 35 additional spellings across
ten families. Byte extraction and repeated sign extension add three rules.
Nine further rules discard masks on bits that shifts or BYTE ignore and adjust
byte indexes across aligned shifts. Index arithmetic must check the
original index before adjustment; a huge index must not wrap into the word.
Byte/shift fusion stays within one block to avoid replacing a temporary with a
longer-lived input across a branch. This placement guard is overapproximated in
the proof; correctness does not depend on its implementation.
Constant SHL, SHR and BYTE constructors call the same EVM evaluator as constant
folding. The verifier models their full-width indexes independently and records
the evaluator's source hash in the trusted boundary.
All compiled word rules pass the same source-based verification. Arithmetic and
bitwise rewrites become competing e-class alternatives. Bounded operand matching can inspect up
to four retained child spellings, one at a time, exposing nested opportunities
within one pass. It preserves instruction placement and dominance; it does not
form the Cartesian product of child classes or perform unrestricted saturation.

Lossless-shift rules cancel a left/right shift pair and compare unshifted
operands when range analysis proves that neither left shift loses bits. Constant
comparisons additionally require alignment to the shift. The verifier models
the range guard as a word-level mask contract and checks every shift count;
mutation tests expose counterexamples when essential guards are removed.
Cancellation requires one original use and stays within one block to avoid
extending a value's lifetime across control flow or changing shared overflow
checks that affect later outlining. Comparison alternatives still compete under
the existing cost model; independently priced consumers may retain a shared
shift even when changing all of them together could remove it.

Extraction does not charge an input's computation again when every alternative
of a shared displaced producer directly needs that input. It still charges
for the extra stack copy. Literals are fresh pushes, so their reuse does not
trigger a live-value preservation charge. This allows some shared comparisons
to use smaller operands without a new analysis or a global extraction search.
The estimate checks one dependency edge and uses original use counts; scheduled
bytecode and runtime measurements still decide whether the change is useful.

The `word-sequence` pass follows e-graph extraction. Its rules cover De Morgan
identities, boolean tests, common masks, common shifts and size-oriented power-of-two
comparisons. A recipe has a private namespace; only a winning recipe allocates MIR
instructions. Matching is restricted to earlier producers in the same pure segment.
Dead-code credit requires single-use producers, excludes ABI validation, and stops
at shared values and a bounded cone. The root retains its value identity and semantic
metadata; new children inherit only source context. Unsupported fork opcodes reject
the candidate. This adds local multi-instruction construction, not global placement
search or equality saturation.

The physical selector now checks 15 identities, including reuse of resident
bitwise components. Actual operand preparation, cleanup and residual layouts are
compared before selection. This is separate from the recipe pass's tree estimate.

## Economics and acceptance

Search prices are exported from `Target`, including fork availability, gas,
PUSH0, immediate bytes and code deposit. Regenerate the checked snapshot after
changing the model:

```sh
SNAPSHOTS=overwrite cargo nextest run -p solar-codegen word_rule_costs
```

Discovery supports gas-first, size-first and lifetime objectives. For example,
`--objective lifetime --runs 1` and `--runs 10000` trade deployment bytes against
expected executions. These are tree estimates with resident variable inputs,
not estimates of a complete scheduled program. The actual e-graph extraction
and stack scheduler still price liveness and physical stack work using `Target`.
Offline estimates alone must never justify integration.

For each candidate: verify the actual source, inspect a one-pass MIR regression,
run boundary execution tests, and compare unchanged test IDs on the UI and
runtime corpora under gas and size objectives. Retain baseline JSON and compare
hot gas labels, generated bytecode and compiler timing according to `AGENTS.md`.
Broader motion, sharing, inlining search and whole-program proofs remain separate
work; this lane does not establish their safety or profitability.

For example, bounded matching exposed a constant ABI encoding size that the
existing allocation pass could then place statically. The encoder had already
written the bytes through the heap frontier, so moving the reservation changed
the returned data. Dynamic encoding reservations now retain that address; a
reduced MIR test and the empty-string runtime test cover the interaction. Pure
word proofs alone would not detect it.

The design draws on [Cranelift's acyclic e-graphs](https://bytecodealliance.org/articles/cranelift-progress-2022),
[VeriISLE](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/isle/veri/README.md)
and [Ruler](https://uwplse.org/ruler/). Word semantics follow the
[Ethereum execution specifications](https://github.com/ethereum/execution-specs/tree/master/src/ethereum/forks/cancun/vm/instructions)
and use [Z3 bitvectors](https://microsoft.github.io/z3guide/docs/theories/Bitvectors/).
