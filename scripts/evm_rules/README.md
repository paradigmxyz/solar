# Verified EVM word rewrites

This is an offline search and SMT verification lane for the **actual ISLE source
compiled into the optimizer**. It currently gates `word.isle`, `word_sequence.isle` and
`stack_select.isle`, plus the physical rules in `stack_peephole.isle` and
`late_word.isle`. The compiler
itself has no solver dependency.

```sh
uv run scripts/test_evm_rules.py
uv run scripts/verify_evm_rules.py verify \
  --output target/evm-rules/proofs.json --artifacts target/evm-rules/smt
```

The script pins Z3 through its inline dependency metadata. Each result records
the source and rule hashes, solver version, instruction-selection and extractor
hashes, verifier implementation hash, trusted Rust source hashes, and any
trusted extractor contracts. The optional artifacts are standalone SMT-LIB counterexample queries. An equivalent
rule produces `unsat`. Difficult single-count shift queries can be split into
257 exhaustive cases: each count from 0 through 255 and the entire saturating
range. The checker also verifies partition coverage. Every case must finish
with UNSAT within the partition budget; partial coverage never proves a rule.
The report lists every saved query; replay one with `z3 path/to/rule.smt2` or
`cvc5 --lang smt2 path/to/rule.smt2`.

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

The model uses 256-bit bitvectors, wrapping arithmetic, full-width saturating
shift counts, unsigned comparisons and two's-complement signed comparisons.
DIV, SDIV, MOD and SMOD return zero for a zero divisor. SDIV rounds toward zero
and wraps the minimum signed word divided by minus one; SMOD takes the dividend's
sign. ADDMOD and MULMOD use a 512-bit intermediate. BYTE and SIGNEXTEND check the
full index before shifting. Only literal exponents are currently modeled for EXP.
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

Memory, storage, calls, exceptions, gas observability, stack bounds, code motion
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
uv run scripts/verify_evm_rules.py verify crates/codegen/isle/egraph.isle \
  --timeout-ms 1000 --output target/evm-rules/audit.json
```

That audit is incomplete: nonlinear/variable-shift queries can time out, and
memory/environment terms and some analysis predicates are unsupported. The audit
therefore exits nonzero. Add semantics and tests before moving such rules into
the mandatory proof lane; do not ignore unknown or unsupported results.

## Discovering candidates

Mine candidates from actual MIR artifacts before searching:

```sh
uv run scripts/verify_evm_rules.py mine \
  target/codegen-bench/baseline/artifacts/*/solar/mir.mir \
  --max-ops 8 --max-seeds 128 \
  --output target/evm-rules/mined.json \
  --emit-seeds target/evm-rules/mined-seeds.json
uv run scripts/verify_evm_rules.py discover \
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
uv run scripts/verify_evm_rules.py discover \
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
uv run scripts/verify_evm_rules.py discover \
  --seed-expressions scripts/evm_rules/seeds.json \
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
uv run scripts/verify_evm_rules.py discover \
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
