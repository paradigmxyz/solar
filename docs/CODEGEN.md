# Codegen operations, rules, and costs

## Operation Schema and ISLE Rules

Declare each MIR operation once, as a row in `crates/codegen/src/mir/op_schema.rs`:
typed payload with named tuple operands, mnemonic, result kind, operand type
contract, phase set, effect, traits, and side-effect flag. The macro generates
operand traversal, the `Op` rewrite view, the ISLE prelude, a constructor per
operation (the text parser uses it for every operation built from value
operands alone), and a `FunctionBuilder` method per variant marked
`#[builder(name)]` or `#[builder(name, void)]`. Write custom text syntax or a
custom builder only for an attribute the generic forms cannot express.
Declare EVM opcodes the same way in `backend/evm/op.rs`, with traits and
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
affect execution also need runtime or differential tests. The offline
word-rule checker, its Lean model and proofs, and their tests live in
`scripts/evm-rules/` (see its [guide](../scripts/evm-rules/README.md) for
commands and coverage limits). The checker must reject unsupported semantics,
never treat them as proved.

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
between equivalent code shapes. Each opcode row in `backend/evm/op.rs` has a
`gas(tier)` column. `GasTier::gas` resolves a tier to the selected EVM
version's static gas across the fork schedule (EIP-150, EIP-1884, EIP-2929,
and later repricings), `dynamic_gas` gives the per-word or per-byte part, and
`op_table.snap` snapshots the resolved schedule. `GasTier::fixed_gas` serves
version-independent tiers in constants.

`Target::new(gcx)` builds the model from the session's EVM version,
optimization mode, and optimizer runs. It answers opcode and MIR operation
costs as `Cost { gas, bytes }` (sizing dynamic work from known immediate
operands), push materialization cost, deposit economics
(`CODE_DEPOSIT_GAS_PER_BYTE`, `lifetime_gas`), and objective ordering
(`objective_key`, `cmp`, `improves`).
The stack scheduler plans against a `Target` and orders plans by
`objective_key`; `target/stack.rs` holds its fixed opcode and frame-sequence
estimates, built from the opcode table. Never write gas or byte literals in
passes, the stack scheduler, or the backend: add a tier or query to the model
and pin it with a unit test there.
