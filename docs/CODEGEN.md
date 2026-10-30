# Codegen operations, rules, and costs

## Operation Schema and ISLE Rules

Declare each MIR operation once, as a row in `crates/mir/src/op_schema.rs`:
typed payload with named tuple operands, mnemonic, result kind, operand type
contract, phase set, effect, traits, and side-effect flag. The macro generates
operand traversal, the `Op` rewrite view, the ISLE prelude, a constructor per
operation (the text parser uses it for every operation built from value
operands alone), and a `FunctionBuilder` method per variant marked
`#[builder(name)]` or `#[builder(name, void)]`. Write custom text syntax or a
custom builder only for an attribute the generic forms cannot express.
Declare EVM opcodes the same way in `crates/evmir/src/op.rs`, with traits and
availability per row; `op_table.snap` snapshots the table. Add operations to
these tables only; never add a parallel `match` that classifies them.

Write rewrite rules in ISLE
([tutorial](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift/isle#tutorial),
[DSL reference](https://github.com/bytecodealliance/wasmtime/blob/main/cranelift/isle/docs/language-reference.md))
under `crates/codegen/isle/`: `mir/` for MIR optimizations, `mir-to-evm/` for
instruction selection and stack-aware lowering, `evm-ir/` for physical EVM IR
rewrites.

- Never hand-edit generated files. The schema generates `mir/prelude.isle`
  and `mir/extractors.isle` (value-definition extractors from
  `Op::isle_extractors`); after changing it, refresh and commit both with
  `SNAPSHOTS=overwrite cargo nextest run -p solar-codegen isle_prelude`. The
  opcode table generates `evm-ir/prelude.isle`, one `$OPCODE` constant per
  opcode byte (`cargo nextest run -p solar-codegen evm_isle_prelude`).
- Group rule files by pass (`mir/egraph/`, `evm-ir/peephole.isle`) and
  register new sets in `RULE_SETS` in `crates/codegen/build.rs`, which
  compiles them. A sibling `isle.rs` module includes the output and
  implements the extractors and constructors the rules call; declare
  Rust-backed predicates and projections in the rule file and implement them
  there.
- MIR rules match the `Op` view of one instruction, so rules on different
  operations never overlap. EVM IR rules match a hand-written `Inst` view of
  each instruction in a window of the block tail; window extractors all
  overlap, so every EVM IR rule needs a priority.
- Rules on the same operation need distinct priorities, which order the
  checks: higher first, default zero. ISLE rejects ambiguous overlaps at build
  time. A `multi` term such as `rewrite` instead returns every matching
  alternative for target-cost selection; priorities are not cost estimates.
- Test a boolean predicate with `(if-let true (pred ...))`; a bare `(if ...)`
  only checks that the call succeeded.
- Keep constant folding, phi merging, and anything needing variable-length
  payloads in Rust; the view shows elided payloads as `Unit`. `simplify`
  rules return a value; `rewrite` rules return an `Op` that `Op::into_kind`
  turns back into an instruction.
- Use the schema's operand order, not Solidity infix order; EVM shifts take
  the count first. MIR integer rules use the operand width, EVM IR rules
  256-bit words; both wrap arithmetic, saturate shifts, and follow EVM
  division by zero.
- Put readable `before => after` pseudocode and any preconditions right above
  every pattern.
- Keep semantic guards apart from profitability guards; query `Target` for
  fork support and costs. Apply results through the metadata-safe rewrite APIs.

These e-graph rules bind values through extractors on the left and build the
replacement on the right:

```lisp
;; x + 0 => x
(rule 2 (simplify (Op.Add x (zero))) x)

;; balance(address()) => selfbalance(), when the target supports it
(rule (rewrite (Op.Balance (current_address)))
    (if-let true (has_self_balance))
    (Op.SelfBalance))
```

`cargo check -p solar-codegen` checks rule types and overlaps. Add pass UI
tests per the [test layout](../AGENTS.md#codegen--mir-pass-tests); rules that
affect execution also need runtime or differential tests. A `run-call` in a
`.evmir` test is both: the runner checks the call's result in an EVM and runs
the call on the test's own text in the EVM IR interpreter, which must agree with
the bytecode the pass's output assembles to (see the
[tester guide](../tools/tester/README.md#evm-ir-interpreter-check)). The offline
word-rule checker, its Lean model and proofs, and their tests live in
`scripts/evm-rules/` (see its [guide](../scripts/evm-rules/README.md) for
commands and coverage limits). The checker must reject unsupported semantics,
never treat them as proved.

To count how often each rule fires on an input, with no rebuild, enable the
`solar::codegen::isle` log target of any binary built with the default
`tracing` feature. The rules run only with the optimizer enabled (`-Ogas`,
`-Osize`, or `settings.optimizer.enabled` in Standard JSON):

```bash
RUST_LOG=solar::codegen::isle=debug solar --standard-json < input.json \
  2>&1 >/dev/null | sed -n 's/.*ISLE //p' | sort | uniq -c | sort -rn
```

Each line is a count, the term the rule belongs to, and the rule's file and
line, such as
`127594 rewrite solar-codegen/isle/mir/egraph/arithmetic.isle line 4`.
The line is 0-based: the `(rule` form is on line `n + 1`. Rules that never
appear never fired on that input. For `rewrite`, a firing means the rule
matched and offered an alternative to the e-graph, not that extraction chose
it. The `egraph`, `word_sequence`, and `peephole` rule sets log; `select` does
not, because the cost model and the inliner call it tens of millions of times.

ISLE does local matching and replacement, not whole algorithms. The e-graph
covers scalar identities and pure expression numbering, overlapping the
pure-expression part of `cse`; it does not replace alias-sensitive
memory/storage CSE, SCCP's executable-edge lattice, range analysis, PRE,
LICM, CFG cleanup, or the stack scheduler. Keep those analyses and placement
decisions in Rust. Migrate bounded local identities that fit the rule
vocabulary; do not translate every Rust rewrite, or remove a pass just
because it also folds expressions. ISLE buys one reviewable rule definition,
typed generated matchers, overlap checks, retained alternatives for cost
selection, and reuse by the offline checker, but guarantees neither
equivalence nor better code.

The `egraph` pass (`mir/transform/egraph.rs`) simplifies MIR and numbers
values: an acyclic e-graph with dominator-scoped hash-consing keeps `rewrite`
results as alternative nodes, merges `simplify` results, and extracts the
cheapest node per class under the target cost model, all at the original
instruction positions. A node costs its opcode's gas and bytes ranked under
the optimization objective, plus stack traffic: immediates cost the pushes
that materialize them, an operand counts only when the node is its sole user,
and a rewrite pays a `DUP` for each non-immediate value it newly reaches
while a displaced operand stays live elsewhere. Without the stack term,
rewrites after memory lowering extend live ranges the stack scheduler spills
and measure as a loss. The pass also merges phis, deletes zero-byte copies,
rewrites branches on boolean zero tests, and runs again after memory
lowering. Extend it with rules in `isle/mir/egraph/`, bounds in `max_bits`,
and stack-traffic terms in `Costs::node`. Opcode prices belong in the gas
schedule; the pass never prices or matches instructions itself.

## Target Cost Model

The target cost model in `crates/codegen/src/target.rs` prices every choice
between equivalent code shapes. Each opcode row in `crates/evmir/src/op.rs` has
a `gas(tier)` column, and `crates/evmir/src/gas.rs` holds the schedule:
`GasTier::gas` resolves a tier to the selected EVM version's static gas across
the fork schedule (EIP-150, EIP-1884, EIP-2929, and later repricings),
`dynamic_gas` gives the per-word or per-byte part, and `op_table.snap`
snapshots the resolved schedule. `GasTier::fixed_gas` serves
version-independent tiers in constants.

`Target::new(gcx)` builds the model from the session's EVM version,
optimization mode, and optimizer runs. It answers opcode and MIR operation
costs as `Cost { gas, bytes, steps }` (sizing dynamic work from known immediate
operands, and counting executed instructions), push materialization cost, deposit economics
(`CODE_DEPOSIT_GAS_PER_BYTE`, `lifetime_gas`), and objective ordering
(`objective_key`, `cmp`, `improves`).
`-Ztune` selects the execution engine: `Target::runtime_gas` is the one runtime price
every ranking uses, gas for `none` and gas plus a dispatch charge per executed
instruction for `interpreter`. `jit` emits no jump a bytecode JIT cannot resolve, because one
unresolved jump makes it drop every resolution in the contract: `Target::packs_wide_jump_tables`
keeps the shifted pick of packed wide jump tables off even with `-Zpack-wide-jump-tables`.
`jit` also unrolls and peels loops from the default 200 runs, except loops that carry more than
three words across the back edge (`Target::unrolls_loops`, `Target::unrolls_loop_carrying`).
Creation code has its own model, `Target::priced_for(true)`, under the gas
objective. It runs once per deployment, and the deployment transaction pays
calldata gas (`byte_gas`: 16 per byte, 68 before Istanbul) for each byte instead
of the code deposit, so costs rank by deployment gas and loop copying stays off.
A pass builds the `Target` of the function it is optimizing, never one for the
whole module. MIR function passes ask the function's `FunctionAnalyses`:
`target` for passes that rank code shapes without weighing blocks by how often
they run, which prices creation code only without loops, recursion, or a looping
caller (`Module::creation_pricing_functions`), and `loop_aware_target` for
passes that multiply by trip counts themselves, which price every creation-only
function (`Module::creation_only_functions`) as creation code. The inliner counts
the calls that creation code holds on its own, since it carries its own copy of
each callee, and only adds inlines to those the runtime model chooses. EVM IR
passes ask the block (`Block::target`): creation code is the module that the
runtime follows (`code_follows`), and a block in a loop, or in creation code that
a loop or recursion calls (`BlockMetadata::repeats`, set from
`Module::repeated_functions`), keeps the runtime model. The stack planner does
the same per function. Helpers shared across call sites (`lower-mcopy`,
`lower-abi-encode`, `outline-reverts`, `specialize`) and lowering from HIR keep
the runtime model, because one decision serves code of both kinds or the code's
function and loops are not known yet.
The stack scheduler plans against a `Target` and orders plans by
`objective_key`; `target/stack.rs` holds its fixed opcode and frame-sequence
estimates, built from the opcode table. Never write gas or byte literals in
passes, the stack scheduler, or the backend: add a tier or query to the model
and pin it with a unit test there.
