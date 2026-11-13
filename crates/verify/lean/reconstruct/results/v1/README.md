# Frozen VC bundle v1: cvc5 reconstruction (`vc_smt`) beside `vc_valid`

Formula-level evidence only. Each theorem is `theorem obligation : <lean>.Valid := by <tactic>`
over SolarVerify.Vc.Obligation.Valid. Nothing here relates the SMT encoding to source or bytecode
semantics.

## Inputs (exact, original locks)

- Exporter: paradigmxyz/solar 7f4e94e065c8e90c2e8d265813533c71cd59c1dc, `cargo build --release -p
  solar-verify` (binary sha256 e3c2eeeaef07dc43f0806bdace058cde4c2c768f93a1b4571f79ed6c53db8e25),
  run from a checkout at /home/hand/wt-inferred as
  `solar-verify --emit-lean vc-exports/<C> crates/verify/examples/<C>.sol`, with z3 4.13.4 and
  cvc5 1.3.1 on PATH.
- Runner, library and locks: alpharush/verify-lean 56ae3f5c3e427509fe59a9ebff59d0c6f497d895
  (`discharge.sh`, `frozen/<C>.sha256`, `SolarVerify`; Smt.lean 2c92f3bc…036e3, Vc.lean
  7275fa35…d7cfe). Against 97a2413 the only change is the `vc_leaf` hook in Discharge/Search.lean.
- Every exported file matches `frozen/<C>.sha256`. Each manifest is byte-identical to
  `frozen/v1/<C>/manifest.json`: Counter 98c3c056…e305, Token 86b2d80e…b2f8, Vault 2589125a…49edb,
  ArrayMax fd6ba812…13241. Each run's `inputs.sha256` equals its lock.
- Backend: this package (lean-smt 6b26285, lean-cvc5 d19a134 with cvc5 1.3.1 in-process; Lean
  v4.35.0), built inside the 56ae3f5 tree so it requires that SolarVerify.

```bash
R=<56ae3f5 checkout>/crates/verify/lean; P=$R/reconstruct   # this package, copied there
(cd $P && lake build)
LP=$(cd $P && lake env printenv LEAN_PATH)
DISCHARGE_LOCK=$R/frozen/<C>.sha256 \
DISCHARGE_BACKENDS='vc_valid=vc_valid;smt=vc_smt' DISCHARGE_IMPORTS=SolarVerifyReconstruct \
DISCHARGE_LEAN_PATH="$LP" \
DISCHARGE_LEAN_ARGS="--plugin=$P/.lake/packages/cvc5/.lake/build/lib/libcvc5_cvc5.so" \
LEAN_NUM_THREADS=1 bash $R/discharge.sh vc-exports/<C> 120
```

## Results (63 required obligations, 15 modules; 120 s per attempt)

| Run | Counter | Token | Vault | ArrayMax | Total | Wall |
|---|---|---|---|---|---|---|
| `smt=vc_smt` | 14/14 | 13/13 | 17/17 | 19/19 | 63/63 | 144 s |
| `vc_valid=vc_valid;smt=vc_smt` | 14/14 | 13/13 | 17/17 | 19/19 | 63/63 (57 vc_valid, 6 smt) | 600 s |

Every one of the 126 proofs depends on exactly `[propext, Classical.choice, Quot.sound]`
(the `axioms` column). smt only: 1.5–3.1 s and at most 1,882 MB per obligation. The combined
results.tsv records the time and peak of the backend that proved each row. vc_valid's failed
attempts on ArrayMax max#8–#10 (120 s timeouts, 9–12 GB each) are in the 600 s wall time, not in
the rows. Per-row files: `smt/<C>.results.tsv`, `combined/<C>.results.tsv`.

The six rows vc_valid leaves open, all proved by smt:

| Row | smt s | MB | Hypotheses (see Vacuity) |
|---|---|---|---|
| Vault.deposit#7 | 2.2 | 1,825 | contradictory: goal `false`, the guarded re-entry path is infeasible by design |
| Vault.withdraw#8 | 2.3 | 1,833 | contradictory: same, for withdraw |
| Vault.withdraw#6 | 2.2 | 1,819 | contradictory: the line-52 assert checked inside that infeasible path; holds vacuously. The same assert on the feasible path is withdraw#0, which both backends prove |
| ArrayMax.max#8 | 3.2 | 1,878 | not refuted (cvc5 incomplete on the quantified hypotheses) |
| ArrayMax.max#9 | 3.3 | 1,872 | not refuted |
| ArrayMax.max#10 | 2.9 | 1,872 | not refuted |

So on the three Vault rows smt beats vc_valid only because vc_valid's prep step hits simp's
recursion limit on contradictory hypotheses. It proves nothing beyond "this path is unreachable".
The three ArrayMax loop-preservation rows are smt's substantive wins.

## Vacuity (`vacuity.tsv`)

For each required row, `{ ob with goal := .bool false }` was tried with `vc_smt`:
- 5 VACUOUS, kernel-refuted with clean axioms: Vault.deposit#6, #7, withdraw#6, #7, #8, all in
  script 1, the "re-entered while `locked`" context. deposit#6/withdraw#7 are `goal := true`
  queries the encoding relies on, and deposit#7/withdraw#8 have goal `false` by design.
- 27 not vacuous (cvc5 finds a model of the hypotheses).
- 31 undetermined (cvc5 `unknown` or 60 s timeout on quantified hypotheses).

## Negatives (`negatives.tsv`): every one fails closed in ≤ 2.6 s, none by timeout

| Case | Mutation | Lock | vc_valid | smt |
|---|---|---|---|---|
| N1 source | Counter.sol `ensures count == old(count) + 2` on increment; exported fresh (solver: sat, required) | n/a | — | increment#1 fails: cvc5 model ("either it is false"), 1.5 s |
| N2 VC goal | Counter increment ob1 goal `+1` → `+2` | fails before any proof | increment#1: omega counterexample | increment#1: cvc5 model |
| N3 VC assertion | script0 definition `count' = count + 1` → `+ 2` | fails before any proof | increment#1 and #2: omega counterexample | increment#1 and #2: cvc5 model |
| N4 negated goal | Vault withdraw ob0 goal negated | n/a | — | withdraw#0 fails: cvc5 incomplete, 2.1 s |
| N4 negated goal | ArrayMax max ob8 goal negated | n/a | — | max#8 fails: cvc5 incomplete, 2.6 s |
| (info) | Vault withdraw ob6 goal negated | n/a | — | **proved**: hypotheses contradictory (above) |

The last row is not a soundness problem: `False` follows from that row's hypotheses in the
kernel, so the formula and its negation are both valid.
