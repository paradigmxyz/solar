# Reconstructing cvc5 proofs of exported VCs

This package proves the obligations `solar-verify --emit-lean` exports (see
docs/VERIFY_LEAN.md) from cvc5's proofs, reconstructed in Lean by
[lean-smt](https://github.com/ufmg-smite/lean-smt) (branch `no_mathlib`, pinned in
`lakefile.lean`). It is a second way to discharge the VCs beside `vc_valid`
(SolarVerify.Discharge), which searches for a proof itself.

```bash
cd crates/verify/lean/reconstruct && lake build        # fetches lean-smt, lean-auto, lean-cvc5
target/release/solar-verify --emit-lean out crates/verify/examples/Counter.sol
bash crates/verify/lean/reconstruct/reconstruct.sh out [TIMEOUT_SECONDS] [ID_REGEX]
```

## How a proof is built

`vc_smt` (SolarVerifyReconstruct/Tactic.lean):

1. `vc_valid_prep`, from SolarVerify.Discharge, evaluates `Obligation.Valid ob` symbolically
   into a shallow goal: `False` under the script's assertions and the guard, over `Int`,
   `Bool` and functions (arrays).
2. `vc_smt_norm` substitutes away local definitions and turns Booleans into propositions,
   conditions of `if` included, so that the quantifiers the export evaluates classically
   (`qdec (∀ x, …)`) reach cvc5 as quantified formulas.
3. lean-smt's `smt` translates the goal to SMT-LIB, runs cvc5 in-process (lean-cvc5's
   plugin) with proofs on, and replays cvc5's CPC proof rule by rule as Lean tactics and lemma
   applications. cvc5 gets 60 s; `sat` or `unknown` fails the proof.

The result is an ordinary Lean proof term that the kernel checks. cvc5, lean-smt's tactic
code and the plugin are untrusted: they can only make a proof fail. A step lean-smt cannot
replay becomes a hole (`sorryAx`); `reconstruct.sh` accepts a theorem only when Lean
exits cleanly and `collectAxioms` reports nothing beyond `propext`, `Classical.choice` and
`Quot.sound`. It also refuses to run if this package's own sources contain `sorry`,
`native_decide`, `bv_decide`, `ofReduceBool` or an `axiom`, and checks that
`Smt.lean` and `Vc.lean` match the hashes in the export's manifest.

## Results (Lean 4.35.0, cvc5 1.3.2 in lean-cvc5)

Required obligations of `crates/verify/examples`, one `lean` process per obligation,
`LEAN_NUM_THREADS=1`:

| Contract | Proved | Seconds per VC (incl. Lean start-up) |
|---|---|---|
| Counter | 14/14 | 1.5–2.3 |
| Token | 13/13 | 1.4–2.6 |
| Vault | 17/17 | 1.5–2.7 |
| ArrayMax | 10/10 | 1.6–3.0 |

All 54 depend only on `propext`, `Classical.choice`, `Quot.sound`; 108 s in all.

Negatives (`Bugs.sol`): each required obligation the solvers answer `sat` fails in about 2 s,
by cvc5 finding a model (`unable to prove goal, either it is false …`) or giving up, never by
a Lean timeout.

Corpus (`crates/verify/tests/cases`): not yet a valid measurement. A first run showed most
failures are term translation, not proof replay: declared functions of arity ≥ 1 and record
fields reach lean-smt as `env.fn f args ret (…)` / `env.1 …`, and leftover `List`,
`List.all` and `DecidableEq` terms of the deep embedding fail its translator; some obligations
with `div`/`mod` or casts fail in `vc_valid_prep` before reconstruction. All fail closed.

## What cvc5's proofs use

cvc5 1.4.2 proves every `unsat` query of the four examples in 7–31 ms (`--produce-proofs`).
One ArrayMax query (`max`, a quantified loop invariant) is `unknown` to cvc5 alone; after
`vc_smt_norm` the corresponding obligation is proved.

CPC proofs (all 4 examples, 52 unsat queries), rules by frequency:

```
cong:1239 trans:1083 eq_resolve:689 nary_cong:661 refl:655 arith_poly_norm:623
arith_poly_norm_rel:349 aci_norm:214 evaluate:200 scope:172 symm:146
bool-double-not-elim:131 and_elim:122 arith-elim-leq:118 chain_m_resolution:104
arith-leq-norm:81 arith-elim-lt:79 process_scope:77 arith-geq-tighten:65 and_intro:53
reordering:46 modus_ponens:41 bool-and-de-morgan:39 arith_mult_neg:35 true_elim:32
not_and:28 implies_elim:28 contra:26 arith_sum_ub:24 resolution:22 bool-impl-elim:22
cnf_or_neg:22 eq-refl:21 int_tight_ub:17 cnf_and_pos:15 arith-elim-gt:13 exists-elim:9
skolem_intro:8 ite_eq:8 ite_elim2:8 arith_trichotomy:7 ite_elim1:6 false_intro:6
skolemize:6 instantiate:6 array-read-over-write:5 cnf_or_pos:5 true_intro:4 eq-symm:4
array-read-over-write-split:3 equiv_elim1:2 bool-eq-false:2 arith-geq-norm1-int:2
false_elim:2 cnf_and_neg:2 not_implies_elim2:2 not_implies_elim1:2 arrays_read_over_write:2
chain_resolution:1 not_not_elim:1 bool-eq-true:1 bool-impl-false1:1 int_tight_lb:1
quant-unused-vars:1 bool-or-taut:1 ite-else-true:1 eq-ite-lift:1
```

The fragment is QF_UFLIA plus quantifiers over `Int` and `Int → Int` arrays: congruence and
equality reasoning, `arith_poly_norm`/`arith_poly_norm_rel` (polynomial normalisation),
Farkas-style `arith_sum_ub`/`arith_mult_*`/`int_tight_*`, the RARE rewrites
(`arith-elim-*`, `bool-*`), CNF and resolution, `skolemize`/`skolem_intro`/`exists-elim`
and `instantiate` for the quantifiers, and `array-read-over-write(-split)`. lean-smt replays
all of them. The arrays reach cvc5 as functions after `vc_valid_prep` beta-reduces `store`,
so the theory of arrays is used only where the export keeps an array equality.

Alethe proofs of the same queries are not usable as certificates: cvc5 1.4.2 writes 2,889
`hole` steps across them (the steps it cannot express in Alethe, mostly normalisation and
rewrites), which a checker would have to trust. CPC has none.

## Bit-vectors: not covered yet

None of the four examples has a bit-vector; `bvlower` emits them only for bitwise operations
and casts (`tests/cases/rewrites.sol`, `speclang.sol`), and Seaport will have many. On these
lean-smt does not yet produce clean proofs:

- bit-blasting steps (`bv_bitblast_step`) are left as unsolved goals even at width 8, and
  lean-smt's `eq_eq_beq`, which the bit-blasting replay uses, is itself a `sorry`;
- mixed terms `(bv2nat x)` fail translation (`BitVec w has variable width`), and `2^256`
  written as a power fails (`incorrect number of universe levels instPowNat`);
- cvc5's default bit-blasting proof of a 256-bit `bv2nat (bvand x c)` query does not finish in
  120 s. With `--solve-bv-as-int=sum` it is `unsat` at once with a 32 KB arithmetic proof,
  but that proof has 2 `trust INT_BLASTER` steps (the translation of each bit-vector term into
  integers), which would need reconstructing.

So bit-vector VCs need either a reconstruction of the int-blaster's steps (lemmas such as
`(x &&& c).toNat = …` for constant masks, provable in the kernel), or VCs that keep such
terms in `Int` before they reach a solver. Pure bit-vector identities (`f &&& g = g &&& f` at
width 256) already reconstruct cleanly through cvc5's rewrite rules.

## Scaling

Per obligation the cost is Lean start-up and the import of lean-smt (about 1.3 s), cvc5
(milliseconds), and the replay, which grows with the proof: the largest CPC proof among the
examples is 65 KB (ArrayMax) and replays in about 1.5 s. Obligations are independent, so they
run in parallel. What does not scale today is the bit-vector fragment above, and replay of
`arith_poly_norm` on wide nonlinear terms, which lean-smt does by normalising in Lean.
