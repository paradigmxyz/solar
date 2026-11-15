# solar-verify loop cuts: the `--emit-loopvc` interface (format 1)

`solar-verify --emit-loopvc DIR FILE.sol` exports, for every verified function with a loop, the
loop's cutpoints and the edges between them as a machine-readable interface: JSON, a constrained
Horn clause (CHC) view in SMT-LIB, and Lean. Each edge's formulas are not written by hand: they
are the obligations solar-verify's own encoder generated for the function on that run, with the
loop's candidate invariant and ranking function abstracted, and a Lean theorem per obligation
template states, proved by `rfl`, that putting the run's candidate back gives exactly the
obligations `--emit-lean` wrote for the run ([LEAN.md](../crates/verify/LEAN.md)).

This document is the contract. Format 1 is frozen: a change to a field's meaning, a file, the
template semantics or `LoopVc.lean` is format 2.

## Running

```text
cargo build -p solar-verify
cd crates/verify
../../target/debug/solar-verify --no-infer --only Counter.bump \
    --emit-loopvc out tests/loopvc/verify_counter.sol
```

`--emit-loopvc DIR` is `--emit-lean DIR` plus the loop-cut files, so `DIR` also holds
`manifest.json` and the obligation modules. Add `--dry-run` to run no solver: the loop-cut files
do not depend on solver answers, and a run with z3 writes the same bytes
(`crates/verify/tests/cases.rs` pins both). Use `--no-infer`: format 1 cuts at the candidate
the source states, and solar-verify's own inference would add conjuncts whose entry checks read
other prefixes of the script (an error, see Limits). Inference belongs to the consumer, over the
Horn view or the templates.

The output for one function `C.f` (module path `C/f`, as for `--emit-lean`):

```text
DIR/LoopVc.lean            the Lean definitions every loop-cut module imports (constant text)
DIR/C/f.lean               the run's obligations (--emit-lean), namespace SolarVerifyVc.C.f
DIR/C/f.loopvc.json        the interface: cutpoints, edges, templates, candidate, revert paths
DIR/C/f.horn.smt2          the CHC view of the templates
DIR/C/f_loopvc.lean        the templates in Lean, with the exactness theorems
```

and the function's entry in `manifest.json` gets `"loopvc": {file, lean, horn, templates}`, or
`"loopvc": {"error": ...}` when the cut cannot be exported; then the run exits 1 and prints
`error: --emit-lean: C.f: --emit-loopvc: <why>`. A unit without a loop gets no loop-cut files.

## The cut

solar-verify proves a loop by cutting it at its head (`crates/verify/src/vc/infer.rs`): it checks
the invariant where the loop is entered, havocs the variables the loop writes, assumes the
invariant once (one assertion of the conjunction of its conjuncts), runs the body once, checks the
invariant and the ranking function (`decreases`) after it, and continues after the loop from the
head state with the guard false. That assumption is the only place the candidate enters the
script; its checks are the only queries whose goals are the candidate. The export replaces both:

- the head assumption `(assert (= G' (and G C)))`, `C` the conjunction, becomes
  `(assert (= G' (and G (|Inv@L0| p1 ... pm))))`;
- a check of conjunct `j` becomes a template whose goal is `(|Inv@L0| a1 ... am)`, the
  predicate at the edge's arguments;
- the ranking check `(and (<= 0 r) (< r' r))` becomes
  `(and (<= 0 (|Rank@L0| p..)) (< (|Rank@L0| b..) (|Rank@L0| p..)))`.

`p1 ... pm` are the cut's parameters: the head constants of the variables of value type (Int or
Bool) the loop writes (role `loop`) or reads (`read`), by name, then the function-entry
constant of each written one (`old`, named `old(x)`). Each has its term where the loop is
entered (`entry`) and after the body (`back`). A candidate is a list of conjunct terms and a
ranking term over the parameters' head constants; instantiation substitutes the arguments for
the parameters.

The exporter checks, and fails otherwise, that: the head assumption is the recorded command of
the recorded script; every check of the candidate is its conjunct substituted at the edge's
arguments, conjunct by conjunct and in order, all on one prefix and guard per edge; the ranking
check is the ranking term at the head and at the back arguments, and the latter is the run's own
term after the body; the candidate mentions no constant but the parameters; evaluating the
candidate declared no symbol (so the rest of the script does not depend on it).

## Cutpoints and edges

| id | kind |
| --- | --- |
| `pre` | function entry (requires and contract invariants assumed) |
| `L0` | the loop head |
| `return` | function exit, outcome `normal` |
| `revert` | function exit, outcome `revert` |

| edge | from, to | templates |
| --- | --- | --- |
| `entry` | `pre`, `L0` | obligations on the way (role `path`), then the invariant on entry (`invariant`) |
| `back` | `L0`, `L0` | obligations in the body (`path`), the invariant after the body (`invariant`), the ranking function (`rank`) |
| `exit` | `L0`, `return` | obligations after the loop: postconditions, contract invariants (`path`) |
| `revert` | `pre` or `L0`, `revert` | none: revert paths, each with its path condition |
| `call` | `L0`, `L0` | always empty in format 1: a loop that calls is an error |

Every obligation of the function (manifest role `obligation`) is an instance of exactly one
template. Outcomes stay separate: templates are about states that reach the head or return
normally; a revert path carries no obligation (solar-verify proves a function's specs on normal
returns; without `succeeds-if` nothing is owed on a revert) and is listed with its condition;
out of gas is not modelled by solar-verify's encoding, where a loop with a ranking function has no
gas path and one without a ranking function reverts (`reverts_if(true)`) on every iteration.
Termination is the `rank` template on the back edge with the `invariant` template: a ranking
function that is non-negative and decreases under the invariant.

## `C/f.loopvc.json`

```text
format, version          "solar-verify-loopvc", 1
unit                     "C.f"
obligations              {module, namespace} of the --emit-lean module the templates instantiate to
lean                     {module, namespace, library: "LoopVc.lean", library_sha256}
horn                     {file, sha256}
cutpoints                [{id, kind, site?, outcome?}]
cut                      {id: "L0", loop: "for" | "while" | "do-while", site, predicate: "|Inv@L0|",
                          rank: "|Rank@L0|",
                          params: [{name, symbol, lean_symbol, sort, role, entry, back}],
                          head: {script, command, guard_before, guard_after, template}}
edges                    [{id, from, to, outcome, templates: [template id]} | {..., paths: [revert id]}]
templates                [{id, edge, role, script, commands, asserts, guard, goal, lean,
                           instances: [{id, lean, kind, site, spec}]}]
reverts                  [{id, from: "entry" | "body" | "exit", script, commands, asserts, guard}]
candidate                {conjuncts: [{term, text, origin, site}], rank: {term, text, origin, site} | null,
                          head_assumption}
sha256                   {formulas, candidate}
```

- `script` and `commands` index the unit's SMT-LIB scripts (`C/f.scriptN.smt2`): a template's
  hypotheses are the script's first `commands` commands, with the head command `cut.head.command`
  replaced by `cut.head.template`, and its guard; `asserts` is how many assertions those are (the
  Lean `upto`). A template holds when `(and hypotheses guard (not goal))` is unsatisfiable for
  every interpretation of `|Inv@L0|` and `|Rank@L0|` that the candidate gives.
- `instances` are the run's obligations (manifest ids and Lean names) the template gives at the
  run's candidate, in order: one per conjunct for an `invariant` template, else one.
- `origin`: `annotation` for a `@custom:invariant` or `@custom:decreases`, `inferred:<how>`
  for solar-verify's own inference.
- `sha256.formulas` hashes the parameters, the head template, the templates without their
  instances and the revert paths: what does not depend on the candidate or the source's layout.
  Two runs of the same function with different candidates (same number of source lines) have the
  same `formulas` hash; `crates/verify/tests/cases.rs` checks this for two candidates of
  `Counter.bump`. `sha256.candidate` hashes `candidate`.
- Sites are `{file, line, column}`, files relative to the run's directory, as in the manifest.

## `C/f.horn.smt2`

`(set-logic HORN)`, the script's `define-fun`s, `(declare-fun |Inv@L0| (sorts) Bool)` and a
nullary `|Reverted|`, then per template except `rank` one clause universally closed over the
constants its prefix declares:

```text
invariant template:  (=> (and hypotheses guard) (|Inv@L0| args))
path template:       (=> (and hypotheses guard (not goal)) false)
revert path:         (=> (and hypotheses guard) |Reverted|)
```

so the predicate occurs only as a conjunct of a body, the head assumption
`(= G' (and G (|Inv@L0| p..)))` is written as `(= G' G)` and the conjunct `(|Inv@L0| p..)`.
That is the same clause when the guard implies `G'`, which the exporter checks on the guards'
definitions for every clause past the head. z3's Spacer answers `sat` on `Counter.bump`'s view,
with a model of `|Inv@L0|`. A `declare-fun` of positive arity or a datatype in a prefix has no
clause form and is an error. The ranking function is not a CHC: use the JSON or Lean template.

## `C/f_loopvc.lean` and `LoopVc.lean`

`LoopVc.lean` (namespace `SolarVerify.LoopVc`, importing solidity-lean's `Verify.Obligation`)
defines `Cut` (the parameters' symbol numbers), `Cand` (`conj : List Term`,
`rank : Option Term`), `Template` (`script`, `base`, `upto`, `guard`, `goal`: the template
script declares the function's `base` symbols, then `Inv` as symbol `base` and `Rank` as
`base + 1`), the substitution `subst`, the instantiation `inst` (an `Inv` application becomes
the conjunction of the conjuncts at its arguments, the term itself for one conjunct, as the encoder
assumes it; a `Rank` application the ranking term), `Template.instances` and
`Template.Valid t cut c := ∀ o ∈ t.instances cut c, o.Valid`.

`C/f_loopvc.lean` (namespace `SolarVerifyLoopVc.C.f`) imports `LoopVc` and `C.f` and defines
`tscriptN`, `cut`, `cand` (the run's candidate) and per template `tK` with

```lean
theorem tK_exact : tK.instances cut cand = [SolarVerifyVc.C.f.obI, ...] := rfl
theorem tK_valid : tK.Valid cut cand ↔ ∀ o ∈ [SolarVerifyVc.C.f.obI, ...], o.Valid :=
  Template.valid_iff tK_exact
```

the bridge: each exported edge formula, at the run's candidate, is the obligation solar-verify's
semantics gave that edge, so discharging the templates' instances (Verify/Discharge) discharges the
function's own obligations, which `checkScript` finds in its script (Verify/VC/EndToEnd). The
theorems use no axiom beyond `propext`, `Classical.choice` and `Quot.sound`. For a different
candidate, `Template.instances` gives the obligations it would have; they are solar-verify's own
for that candidate when a run with it annotated has the same `sha256.formulas` and its module's
`_exact` theorems hold.

Checking a module, with solidity-lean's `Verify.Obligation` built (`SOLIDITY_LEAN` a checkout):

```text
(cd "$SOLIDITY_LEAN" && lake build Verify.Obligation)
export LEAN_PATH="$(cd "$SOLIDITY_LEAN" && lake env printenv LEAN_PATH):$PWD/out"
cd out && lean -o LoopVc.olean LoopVc.lean && lean -o Counter/bump.olean Counter/bump.lean \
  && lean Counter/bump_loopvc.lean
```

`cargo nextest run -p solar-verify emit_loopvc` runs this with `SOLIDITY_LEAN` set (and
`SOLAR_VERIFY_REQUIRE_LEAN=1` to fail without it).

## Limits of format 1

Each is an export error, never a silently missing edge:

- one loop per function (nested or sequential loops: error);
- no call in the loop (call edges);
- the candidate's evaluation declares no symbol, its conjuncts are over the parameters only, and
  the parameters are Int or Bool constants; one `decreases` clause at most;
- solar-verify's inference off (`--no-infer`) or adding nothing: inferred conjuncts are checked on
  entry after the havoc, on another prefix than the annotated ones;
- a loop with no invariant (nothing to cut at) or whose head is unreachable on the run.

Not part of the interface: queries that are not obligations (side, split and advisory queries);
which `require`, overflow or division a revert path comes from (the path condition says it); a
non-payable function's `msg.value == 0`, which solar-verify assumes at entry rather than as a
revert path.
