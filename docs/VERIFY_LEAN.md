# solar-verify's verification conditions in Lean

`solar-verify --emit-lean DIR FILE.sol` writes every query solar-verify asks a solver while it
verifies the file as a Lean term, with a manifest that binds each one to the run that produced
it. The terms are values of a small deep embedding of the SMT-LIB fragment solar-verify writes,
whose meaning is the Lean library in [`crates/verify/lean`](../crates/verify/lean): an
obligation is valid in Lean exactly when the query solar-verify sent is unsatisfiable. This is
how a kernel-checked proof can take the place of the solver for any obligation, and how other
tools can read what solar-verify claims.

This document describes format version 1. A change to the embedding, the file layout or the
meaning of a manifest field is a new version; consumers check `manifest.json`'s `version`.

## Running

```text
cargo build -p solar-verify
target/debug/solar-verify --emit-lean out crates/verify/examples/Counter.sol
cd crates/verify/lean && lake build
lake env lean ../../../out/Counter/increment.lean
```

The export runs the solvers as a plain run does, and records their answers. With `--dry-run`
nothing is asked and every answer is `not-run`. `--emit-lean` turns off `--jobs`. A unit
whose recording cannot be translated is an error (the run exits 1) and its manifest entry says
why: the export fails closed and never drops an assertion it cannot write.

## Coverage

Every example under `crates/verify/examples` exports without errors, and the tests pin what
each exercises:

| Example | Units | Obligations | What it exercises |
| --- | --- | --- | --- |
| Counter | 4 | 18 | state variables, `old`, a contract invariant |
| Token | 4 | 17 | mappings (arrays of the embedding), a ghost sum, constructor invariants |
| Vault | 3 | 20 | a reentrancy lock, `assert`, external calls |
| ArrayMax | 2 | 12 | a loop: invariant on entry and preserved, `decreases`, array reads |

Counts are queries with Z3 4.13 and include split cases, side queries and advisory checks; the
manifest's `role` tells them apart. Export one source file per run: a run over several files
can report spurious ghost-erasure errors in the later ones, which affects verification as a
whole, not only the export.

## Layout

```text
DIR/manifest.json
DIR/<Contract>/<function>.lean             one module per unit
DIR/<Contract>/<function>.script<N>.smt2   the SMT-LIB commands of script N, one per line
```

A unit is a verified function (its reentrancy-lock pass included) or a broadcast lemma, named
`<Contract>.<function> [lemma]`. Module paths keep letters, digits and underscores of the
names, and a second unit of the same name gets a `_2` suffix.

Each module imports `SolarVerify` and lives in namespace `SolarVerifyVc.<Contract>.<function>`.
It defines:

- `script<N> : Script`: the solver's assertion stack: the declared symbols in order
  (`decls`) and the assertions in order (`asserts`). Commands are the ones solar-verify sent
  outside `check-sat`, with every `push` and `pop` applied: a pop that removes commands a
  query read starts a new script.
- `ob<K> : Obligation`: query `K` of the unit, in the order solar-verify asked them, as the
  script, how many of its assertions were in scope (`upto`), the guard and the goal.

## Meaning

[`SolarVerify/Smt.lean`](../crates/verify/lean/SolarVerify/Smt.lean) defines the embedding:

- Sorts (`Srt`): `bool`, `int`, `bv w`, `array idx elem` and `tup fields`. solar-verify
  declares each Solidity struct as a datatype with one constructor; the export writes it as the
  product of its field sorts, its constructor as `mk` and its selectors as `proj i`. A
  recursive, parametric or multi-constructor datatype is an error.
- Values (`Srt.Den`): Bool, Int, `BitVec w`, total functions for arrays (SMT-LIB's arrays are
  extensional), tuples for records.
- Terms (`Term`): literals, declared symbols applied to arguments (`fn f args`, constants
  with none), built-in operators (`app op args`), constant arrays, records, field projections
  and `forall`/`exists` with de Bruijn indices (`var 0` is the last binder of the innermost
  quantifier).
- An interpretation (`Env`) gives every declared symbol a function and fixes what SMT-LIB
  leaves unspecified: integer `div` and `mod` by zero.
- `Term.eval env decls locals t` is the value of `t`, or `none` when `t` or any part of it
  is ill-sorted: a quantifier is `none` unless its body is a formula at every value of its
  binders, and every operator checks its arguments' sorts and number.

The operators follow SMT-LIB 2.6 and Z3: n-ary `and`, `or`, `+`, `*`, `-` (unary is negation),
chainable comparisons and `=`, pairwise `distinct`, right-associative `=>`, Euclidean `div`
and `mod`, `abs`, `select` and `store`, `bv2nat` (Z3's `bv2int`), `(_ int2bv w)`, and the
bitvector operations with SMT-LIB's results for division and remainder by zero. Annotations
(`:pattern` and other attributes) guide a solver and are dropped.

[`SolarVerify/Vc.lean`](../crates/verify/lean/SolarVerify/Vc.lean) defines what an obligation
claims:

```lean
def Obligation.formula (o : Obligation) : Term :=
  .app .implies [.app .and (o.script.asserts.take o.upto ++ [o.guard]), o.goal]

def Obligation.Valid (o : Obligation) : Prop :=
  ∀ env : Env, Holds env o.script.decls o.formula
```

`Holds` says the formula evaluates to `true`, so an ill-sorted assertion, guard or goal makes an
obligation unprovable rather than vacuous. A `define-fun` is a declaration and an assertion
that the symbol equals its body at every argument.

## Manifest

`manifest.json` binds the export to its run:

- `format` (`solar-verify-lean`) and `version` (1).
- `compiler`: the build's solc-compatible version, with its commit; `arguments`: the command
  line; `options`: `strict`, `timeout_ms`, `only`, `dry_run`; `solvers`: the version line of
  z3, cvc5 and bitwuzla, `null` for one not found.
- `sources`: every source file the run loaded, with its sha256.
- `lean`: the toolchain, and the sha256 of `Smt.lean` and `Vc.lean` as the binary was built
  with them, so a proof can check it reads the terms with the semantics they were written for.
- `units`: per unit its `name`, `status` (`verified`, `failed`, `unknown`, `unsupported`,
  `lemma` for a broadcast lemma, or `not-exported` with an `error`), `module`, `namespace`,
  `scripts` (the Lean name, the SMT-LIB file, the command count and the declared symbols in
  order) and `obligations`.
- `errors`: the units that could not be exported.

Each obligation has:

- `id` (`<unit>#<K>`) and `lean`, the full name of `ob<K>`.
- `script`, `commands` (how many of the script's commands were sent before the query) and
  `asserts` (the Lean `upto`).
- `guard` and `goal` as SMT-LIB, and `smt_sha256`: the sha256 of the exact query, the
  script's first `commands` lines, each ending in a newline, then
  `(assert GUARD)\n(assert (not GOAL))\n(check-sat)\n`.
- `role`:
  - `obligation`: a spec obligation, with `kind` (the message a failure prints), `site`
    (file, line, column of the code it is checked at) and `spec` (the clause it discharges, its
    text and location, or `null` for a built-in check such as an assertion or overflow).
  - `side`: a query whose `unsat` answer the encoding relies on, such as whether a reentrancy
    lock is held, or a bound the interval analysis claims.
  - `split`: one case of a case split of obligation `parent`.
  - `advisory`: a query only a warning or note reads.
- `answer`: `unsat`, `sat`, `unknown` (with `reason`), `not-run`, `split` (unknown, then
  proved by the cases that name it as parent) or `settled` (with `reason`): the encoder
  discharged it without a solver, because the goal is `true`, the guard is `false`, the goal
  was assumed at entry over unchanged state, or the interval analysis bounds the value. A
  settled query is exported like any other, so a proof can check that claim too.
- `solver`: which solver answered (`z3`, `cvc5`, `bitwuzla(bv)`, `z3(bv)`), or `null`.
- `required`: whether a function's ✔ rests on the obligation: every `obligation`, and every
  `side` query answered `unsat` or settled. A certificate for a verified function proves
  `Valid` for each required obligation of its unit and of the lemmas it uses.

## What the export does not cover

The export is faithful to the queries; it says nothing about whether they encode the program.
The encoding in `src/vc.rs` and `src/vc/yul.rs` stays in the trusted base (see the
[README](../crates/verify/README.md#trusted-base)): in particular, the interval analysis also
rewrites terms where it shows a value in range (no wrap, no `mod`), which no query records.
Queries solved on the bitvector lowering (`solver` ending in `(bv)`) are exported as the
integer query solar-verify meant; `src/bvlower.rs` claims the two agree when it drops nothing.

## Relation to SolidCore and Loom

The source semantics the refinement proofs use is SolidCore, in paradigmxyz/solidity-lean, and
its verification-condition generator is Loom's weakest preconditions over a model of each
function (`SolidCore/Verify`). This export is not a second generator or program logic: it is
an exact image of what solar-verify asked the solver, so that solar-verify's verification
conditions can be checked against Loom's. The intended bridge is, per function, a theorem that
the required obligations' `Valid` imply Loom's `wp` of the function's model under its spec;
the manifest's declared symbol names (such as `|count@17|` for a state variable's value at
entry) and spec clauses are what map the SMT constants onto the model's storage and arguments.
That check lives in paradigmxyz/solidity-lean (paradigmxyz/solidity-lean#158), on top of the
Loom port to Lean 4.35 there and SolidCore's spec semantics (paradigmxyz/solidity-lean#157).
Until it exists a certified theorem about deployed bytecode rests on the Loom side alone and
trusts nothing exported here.

Proofs of exported obligations meant for certified theorems keep to the same rules as the
refinement proofs: no `sorry`, `native_decide`, `bv_decide` or `ofReduceBool`, and no SMT
oracle. The round-trip tests use `decide +kernel`, which the kernel checks, only on ground
terms.

## Testing

[`roundtrip.py`](../crates/verify/lean/roundtrip.py) checks that the translation and the
embedding agree with Z3: for sampled obligations it draws a random finite model of the script's
symbols, has Z3 simplify each sampled term of the query under it, and checks in the Lean kernel
(`decide +kernel` on `Term.evalBool` with an interpretation from
[`SolarVerify/Model.lean`](../crates/verify/lean/SolarVerify/Model.lean)) that the exported term
has the same value. Terms Z3 does not reduce to a literal are skipped. A flipped expectation
must fail by a mismatch the kernel computes. The `emit_lean_round_trip` test runs it on
Counter, Token, Vault and ArrayMax when z3 and Lean are installed, and elaborates every module
it exported with `lake env lean`. Terms under a quantifier or comparing arrays are counted as
`classical` and skipped, since their value is not computable (15 of about 630 terms on these
examples). `emit_lean` and `emit_lean_examples` need neither: they snapshot
`Counter/increment`, `Token/transfer`, `Vault/withdraw` and `ArrayMax/max` from a
`--dry-run` export, and check that the manifest names every function and the spec clause or
loop obligation each query discharges.
