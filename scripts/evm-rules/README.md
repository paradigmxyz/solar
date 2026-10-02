# Verified EVM word rewrites

This is an offline search and verification lane for the **actual ISLE source
compiled into the optimizer**. Under `crates/codegen/isle/`, it checks MIR
rewrites in `mir/word`, `mir/word_sequence` and `mir/egraph`,
lowering rules in `mir-to-evm/stack_select.isle`, and physical EVM IR rules in
`evm-ir/stack_peephole.isle` and `evm-ir/late_word.isle`. Each rule becomes a
theorem about EVM semantics written in Lean 4, and Lean proves it; no SMT
solver takes part. The compiler itself has no prover dependency.

```sh
uv run scripts/evm-rules/test.py
uv run scripts/evm-rules/verify.py verify --output target/evm-rules/proofs.json
```

Install [elan](https://github.com/leanprover/elan), which selects the toolchain
pinned in `lean/lean-toolchain`; `verify` builds the Lean project with `lake`
before checking anything. The rule readers in `evm_rules/isle.py`, `late.py`
and `stack.py` produce solver-independent words and preconditions
(`evm_rules/expr.py`). `evm_rules/lean.py` states each rule as a theorem over
the definitions in `lean/EvmRules/Word.lean`, and every theorem is checked in
its own `lean` process with its own time limit. `--timeout-s` sets the SAT
limit, and each rule may run twice that, plus 30 seconds; `--jobs` sets the
parallelism and `--work-dir` keeps the checked theorem files. The report
records each rule's status, proof method, time and witness, the source and rule
hashes, the Lean version, and hashes of the Python and Lean implementation and
of the trusted Rust sources.

A rule with preconditions must also be applicable: its file states that the
preconditions contradict each other, and that theorem must fail with an
assignment that the independent integer evaluator in `expr.py` confirms. Only a
proof establishes equivalence. A failed proof whose counterexample replays in
the integer evaluator is reported as that counterexample; timeouts, unsupported
terms and contradictory preconditions are distinct failures, never proofs.
Verification exits nonzero unless every selected rule is proved, and empty rule
files fail too. Failures print the source file, rule line, status and reason.

## CI and local cache

CI runs one proof job on a larger Depot runner. On pull requests it runs only
when codegen, proof tooling, or their CI and dependency inputs change; main
pushes always run it. The exact paths and schedule live in
[ci.yml](../../.github/workflows/ci.yml). The [proof
runner](../../.github/scripts/run_evm_proofs.sh) checks every selected rule with
one `lean` process per theorem on every core. Reports and theorem files live
under `target/evm-rules/`.

```sh
# Verify all selected rules, reusing proved theorems.
bash .github/scripts/run_evm_proofs.sh

# Check every rule afresh.
PROOF_AUDIT=true bash .github/scripts/run_evm_proofs.sh target/evm-audit

# Reuse the same cache for a selected file.
uv run scripts/evm-rules/verify.py verify crates/codegen/isle/mir/word \
  --cache-dir target/evm-proof-cache --output target/evm-rules/selected.json
```

The cache stores only proved theorems, keyed by the exact theorem file,
including any hand-written proof, and a digest of the Lean library and
toolchain. A changed rule, proof, lemma or toolchain misses, and failures,
timeouts and unknown results are never reused. Normal CI restores the latest
cache and saves successful runs; scheduled runs and the `proof-audit` dispatch
option, enabled by default, check every rule afresh. `--cache-dir` or
`SOLAR_PROOF_CACHE` selects the directory; omit both for a fresh run.

The shared `Python` CI job installs the same toolchain and runs this project's
unit tests alongside all other Python suites. Run `bash scripts/check-python.sh`
from the repository root for the same formatting, lint, type checks, and tests.
See [Python tooling](../../AGENTS.md#python-tooling) for prerequisites.

## Lean model and proofs

`lean/EvmRules/Word.lean` defines every operation the readers model on
`BitVec 256`, following the execution specifications: EVM division by zero,
shifts with the specification's word-width case, `SIGNEXTEND` one byte at a
time, `CLZ`, `ADDMOD` and `MULMOD` at 512 bits, and one executing account whose
balances are a function from addresses to words. Each rule becomes a theorem
over these definitions, named after its file and line, whose hypotheses are
the rule's preconditions and whose conclusion is `lhs = rhs`. Preconditions
keep the readers' trusted extractor contracts; Boolean flags that a rule forces
are substituted first. Physical stack rules are checked at every legal depth,
and variants that differ only in variable names share one theorem.

`evm_decide` unfolds the definitions, rewriting the shifts by proven lemmas to
Lean's saturating shifts, and bit-blasts the goal with `bv_decide`, which
checks the SAT solver's LRAT certificate in Lean through `Lean.ofReduceBool`.
A script in `lean/proofs/` replaces that tactic for one rule, with the lemmas in
`lean/EvmRules/Lemmas.lean`. Its file is named after the rule's source file and
the first 16 hex digits of the rule's digest (`egraph_44d329ef52b3f648.lean`),
plus `_<index>` for a rule with several theorems, so edits elsewhere in the
file do not move it. Its statement is still generated from the current rule,
so a changed rule leaves the script without a rule and fails the run, and
`test.py` checks every script against its rule; a script for a selected file
must name one of its rules. A proof that avoids `bv_decide` relies on Lean's
kernel alone.

The trusted base is the readers and their contracts, the Lean model, and the
printer from terms to Lean. Regression tests evaluate every Lean operation
against the integer evaluator on boundary and random words, check printed
preconditions the same way, require false rules to fail with replayed
counterexamples, and require the tools to run without an SMT solver installed.

On 2026-10-02, with 10 jobs and a 120-second SAT limit, the lane proved all 444
selected rules in 156 seconds of wall time and 1,109 seconds of user CPU: 415
with `evm_decide` (median 0.55 seconds, slowest 39) and 29 with hand-written
scripts (at most 0.62 seconds each). Without the scripts, `evm_decide` proves
424 rules, nine of them only after 66 to 205 seconds. It cannot prove the five
`EXP` rules, whose symbolic exponents it cannot bit-blast, or a balance rule
whose reads at two addresses that a guard makes equal it treats as unrelated
words, and it times out on 14 more, mostly division, remainder and shifts by
symbolic amounts. All 161 rules with preconditions have a confirmed witness,
and the 918 physical stack variants reduce to seven theorems. The Z3 and cvc5
lane this replaces proved the same rules in 101 seconds of wall time and 583
seconds of user CPU as 13 workers, with index and output-bit partitions for
the rules no solver finished whole.

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
balances are an arbitrary function from 160-bit addresses to 256-bit words.
`BALANCE` selects the low 160 bits of its operand, and `SELFBALANCE` selects
the executing account. `ADDRESS` zero-extends the address to one word. This
matches the operation definitions in [EIP-1884](https://eips.ethereum.org/EIPS/eip-1884)
and the [execution client's BALANCE implementation](https://github.com/ethereum/go-ethereum/blob/master/core/vm/instructions.go).
The reader still checks opcode selection and records the trusted
`current_address` extractor and fork-availability contracts.

Theorems quantify over the executing address and the balance function, and the
printer rejects every other uninterpreted operation. `bv_decide` treats each
balance read as an unrelated word, so it proves a balance rule only when both
sides read the same account term; the balance-mask rule has a hand-written
proof. To find a counterexample, the checker replaces each read by a fresh word
with `account_i = account_j → balance_i = balance_j`, which describes exactly
the snapshots, then records the current address and every observed account
balance and replays the complete expressions with integer addresses and a
concrete dictionary, including nested balance reads. Tests reject an incorrect
replacement for another account and check upper-bit truncation.

This proves returned-word equality within one state snapshot. It does not
model calls, balance mutations, gas, access-list warming, out-of-gas behavior,
or opcode availability on a particular fork. It must not be used to justify
balance CSE across calls or code motion across state changes. Unmodeled state
operations continue to fail, rather than becoming unconstrained functions.
The ISLE reader permits balance reads only at the instruction roots being
replaced. It rejects nested balance producers, which could have executed before
an intervening call; a shared-state assumption is not silently added for them.

Classic `CALL`, `CALLCODE`, `STATICCALL`, and `DELEGATECALL` rewrites use a
separate effect-preservation obligation. Both sides must keep the same opcode
and every effective operand, with the address truncated to 160 bits. The checker
compares the complete operand tuple; it never models the call result as a pure
value. Nested calls and rewrites that remove or change the call are rejected.
The rewrite driver keeps the instruction at its original position. Gas accounting
and the callee's execution remain outside this proof.

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
for every index at or above 31; this avoids variable shifts when bit-blasting.
A regression proves it equal to the shift-based definition at every index
below 31, and the identity at every larger index.
DIV, SDIV, MOD and SMOD return zero for a zero divisor. SDIV rounds toward zero
and wraps the minimum signed word divided by minus one; SMOD takes the dividend's
sign. ADDMOD and MULMOD use a 512-bit intermediate. BYTE and SIGNEXTEND check the
full index before shifting. EXP is the power modulo 2^256 for every base and
exponent; `bv_decide` cannot bit-blast a symbolic exponent, so the `EXP` rules
have hand-written proofs. The tests evaluate the Lean model and the independent
integer evaluator at these boundaries;
`tests/ui/codegen/mir/egraph/word_rules_runtime.sol` also checks actual compiled
execution, including cases where ordinary integer identities are wrong.

The reader expands the schema-generated `extractors.isle`, reads each rule and
its `if-let` guards, and checks scalar operation/operand-shape bindings against
`select.isle`. A changed or unmodeled opcode selection fails closed. We do not
maintain a second list of optimization identities for verification. Structural
inequality of ValueIds never implies inequality of their runtime words.

This is a proof of **word equality under the model and stated contracts**.
Lean's kernel checks every proof, and `bv_decide` proofs also trust Lean's
compiled LRAT checker. The Lean model, reader, printer, generated bindings,
Rust extractors/constructors and backend implementation are trusted. Range
predicates are conditional contracts, not proofs of the Rust analyses that
implement them. Resident-value selection
assumes the original expression has already executed and remains available.
The new `mir/word/` rules need no range-analysis predicates.
The older `has_known_sign_bit` contract means bit 255 is set; its false result
does not imply the bit is clear. The Rust extractor is conservative and remains
part of the trusted, fingerprinted implementation.

Memory contents, storage, calls, exceptions, gas observability, stack bounds, code motion
and whole-program correctness are outside this proof. ISLE priorities affect
matching but not an individual rule's equality obligation. We overapproximate
structural/fork guards; actual opcode availability and profitability remain the
compiler's responsibility. There is no claim of a verified compiler.

The physical stack lane checks the seven compiled rules in `stack_peephole.isle`
directly, including every supported DUP/SWAP depth from 1 through 235 and every
legal EXCHANGE pair and the EQ/ISZERO shuffle cleanup. It compares every touched
word, the final height, required input depth and peak growth; an arbitrary deeper
prefix stays unchanged.
Malformed input bytecode and out-of-gas behavior are excluded. The Rust window
facets, edits and target lowering remain trusted and are recorded by hash.
The guarded five-op window rejects protected instruction boundaries; focused
window-helper tests check this guard.
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
doubling rule in `mir/word/` and odd-word recipes in `mir/word_sequence/`.
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
representative substitution requires a Lean proof; replayed counterexamples
refine the sample buckets, and unknown results keep separate representatives.
Cheaper proved spellings replace expensive representatives so enumeration can
build on them. One `evm_check` process (`lean/Checker.lean`) answers every
query: it imports the model once, then takes milliseconds per small query.
`--timeout-ms` sets the SAT limit per query, rounded up to whole seconds. This
is bounded enumerative search inspired by cvec-based discovery, not a port of
Ruler or unrestricted equality saturation.

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
the expression budget is exhausted. Samples only select proof queries; every
seed replacement and its emitted ISLE must still be proved. Counterexample
refinement extends cached sample vectors before reusing bucket keys. A seed
with no proved cheaper match is left unresolved, including timeouts.
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
ordinary rules in `mir/word/` and recipes in `mir/word_sequence/`; a mixed
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
and use Lean's `BitVec` library, proved with
[`bv_decide`](https://lean-lang.org/doc/reference/latest/Tactic-Proofs/Tactic-Reference/#bv_decide).

MIR rule directories contain modules grouped by root operation. Pass a directory
to verify all its modules, or an individual `.isle` file for a focused check.
Each result records the actual module path and line, and a theorem's name
includes its module. See [the module layout](../../crates/codegen/isle/mir/README.md).
