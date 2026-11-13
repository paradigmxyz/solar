# solar-verify through the certified chain

Each example under [crates/verify/examples](../crates/verify/examples) is driven through every stage of the chain from its
specs to its deployed bytecode:

1. **spec**: solar-verify proves the example's NatSpec specs with Z3 (SMT, not kernel-checked).
2. **VCs in Lean**: the verification conditions checked by Lean's kernel, from SolidCore's
   Loom-based VC generator (solidity-lean, `SolidCore/Verify`).
3. **source stage**: SolidCore's semantics of the source refined by solar's first lowered MIR
   (solidity-lean's generic source stage, `SolidCore/Refinement/Generic`).
4. **backend stages**: the first lowered MIR refined by the final EVM IR, pass by pass, and the
   final EVM IR's decoding against the bytecode (solar-tv,
   [certify.sh](../solar-tv/milestone/certify.sh)).
5. **assembly stage**: the final EVM IR refined by the bytecode under EthLib's EVM
   (solidity-lean's generic assembler check).
6. **composed**: one theorem from the spec to the bytecode, bound to the manifest's hashes.

A stage is **certified** only when Lean's kernel checks it, `#print axioms` shows no more than
`propext`, `Classical.choice` and `Quot.sound`, and nothing it uses has `sorry`,
`native_decide`, `bv_decide` or `ofReduceBool`. Every negative test must fail by a mismatch or
a failed proof, never by a timeout.

## Status

| Example | spec (Z3) | VCs in Lean | source stage | backend stages | assembly stage | composed |
| --- | --- | --- | --- | --- | --- | --- |
| Counter | ✔ all 4 functions | ✔ solar-verify's 14 VCs kernel-checked (not yet Loom, G6) | blocked (B1, G5) | **certified** except `bump`'s two MIR stages (G1) | blocked (B1) | open |
| Token | ✔ all 4 functions | open | open | not run yet | blocked (B1) | open |
| Vault | ✔ all 3 functions | open | open | ✘ exporter rejects it (G3) | open | open |
| ArrayMax | ✔ both functions | open | open | not run yet; loop (G1) | open | open |
| struct_slot | ✔ f, g, h; ✘ `bad` as written | open | open (assembly) | not run yet | open | open |
| Bugs (must fail) | ✘ 6 of 8 functions, each with a counterexample | n/a | n/a | not run yet | n/a | n/a |

"open" means nobody has run it yet; "blocked" names what stops it.

### Counter

Backend stages: `SolarTV.Milestone.VerifyCounter.verify_counter_mir_to_evmir` in
[solar-tv/milestone/verify_counter](../solar-tv/milestone/verify_counter), for every oracle,

    Stage (mirSem o) (evmirSem o) stage0 evm11 scope

where `scope` is every call whose selector word is not `0xb20eb4c4` (`bump(uint256)`): any
other calldata, any call value and any storage. That covers `increment()`, `max(uint256,uint256)`,
the getters `count()` and `limit()` and every call that matches no selector. Fourteen stages,
`stage0 → mir1 (lower-assumes) → sched → evm1 … evm11`, each by `sym_exec`, and `walk_eq` and
`decodes` for the final EVM IR against the 496 bytes of runtime bytecode
(sha256 `d74a847b…214cc1`, `-Ogas --evm-version osaka`). Axioms:

    verify_counter_mir_to_evmir, stage0_ends, mir1_run, sched_run, evm1_run … evm11_run:
      [propext, Classical.choice, Quot.sound]
    walk_eq, decodes: [propext, Quot.sound]

EVM IR stages through cut points, for every call including `bump`:
`SolarTV.Milestone.VerifyCounter.evmir_stages_all` in
[Proofs/Loops.lean](../solar-tv/milestone/verify_counter/Proofs/Loops.lean),

    Stage (evmirSem o) (evmirSem o) sched evm11 anyCall

chains the eleven EVM IR passes, each proved by `stage_of_certified` from a simulation whose
cut points are the loop headers ([cutstages.py](../solar-tv/milestone/cutstages.py) writes one
module per stage from [cutpoints.py](../solar-tv/milestone/cutpoints.py)). Axioms: `[propext,
Classical.choice, Quot.sound]`. Still scoped to `scope`: `stage0 → mir1` (`lower-assumes`,
which changes only `bump`) and `mir1 → sched` (stack scheduling), since cut points do not yet
relate a MIR loop header's values to the scheduled stack (G1).

VCs in Lean: `solar-verify --emit-lean` writes each of Counter's obligations as a Lean goal over
SolarVerify's embedding and [discharge.sh](../crates/verify/lean/discharge.sh) proves them in the
kernel: all 14 required obligations (constructor 2, `increment` 2, `bump` 7, `max` 3), axioms
`[propext, Classical.choice, Quot.sound]`. This is solar-verify's VC generator, not SolidCore's
Loom one, so column 2 is not yet the bridge G6 asks for.

Build:

    CARGO_TARGET_DIR=... LEAN_NUM_THREADS=1 LEAN_MEMORY=3000 \
      solar-tv/milestone/certify.sh crates/verify/examples/Counter.sol Counter \
      --name verify_counter --exclude 'bump(uint256)'
    (cd solar-tv && python3 milestone/cutstages.py milestone/verify_counter/gen/Counter \
      --gen-module verify_counter.gen.Counter --proof-dir milestone/verify_counter/Proofs \
      --proof-module verify_counter.Proofs --namespace SolarTV.Milestone.VerifyCounter &&
      lake build verify_counter.Proofs.Loops -KleanMemory=3000)
    LEAN_MEMORY=3000 solar-tv/milestone/negative.sh verify_counter Counter
    solar-tv/milestone/verify-chain/negatives.sh Counter

Negatives, every one failing by a mismatch:

- backend stages ([negative.sh](../solar-tv/milestone/negative.sh)): the bytecode's first
  ADD as SUB (the walk evaluates to false); in the final EVM IR the first add as sub, the first
  sstore's key plus one, the call value check dropped and the overflow check dropped; in the
  scheduled EVM IR the first add as sub (each: `sym_exec: the runs end in different results on
  this path`). `lower-assumes` (`stage0 → mir1`) changes only `bump`, so it has no edit inside
  the scope: only its control runs. Through cut points, the last `push 1` of the loop's step
  block in `evm11` (`bump`'s `i++`) as `push 2` fails `stage_evm11_all` (`sym_seg: the segments
  end differently on this path`).
- VCs in Lean (stage 2 of verify-chain/negatives.sh): the control discharges all 14 obligations;
  each of the ten mutants below leaves one to three obligations whose `omega` proof fails.
- spec ([verify-chain/negatives.sh](../solar-tv/milestone/verify-chain/negatives.sh), mutants in
  [verify-chain/Counter/mutants.py](../solar-tv/milestone/verify-chain/Counter/mutants.py)): five code mutants (`count += 2`, the
  loop to `i <= n`, `bump`'s `require` dropped, `max` returning `a`, the constructor storing
  `_limit + 1`) and five spec mutants (`increment` ensuring `+ 2`, the invariant as `count <
  limit`, `max` ensuring `result > a`, `increment`'s requires dropped, the loop invariant off by
  one) each fail solar-verify with a counterexample.
- `solar-verify mutate`: 12 of 14 killed. The survivors are `bump`'s `<=` as `<` (the specs do
  not say `bump` succeeds when the sum reaches the limit) and `max`'s `>=` as `>` (the same
  result; solc's bytecode differs, so it is not counted as equivalent).
- binding: the manifest swapped for the `counter` milestone's, one byte of the source, one byte
  of `Stage0.lean` and one byte of the bytecode pin each fail the hash check.

## Gaps

Each gap names the stage and the construct, and how the stage fails on it.

- **G1** (backend stages, loops whose bound the calldata picks: `Counter.bump`,
  `ArrayMax.max`, `Bugs.countTo`). The run proofs are symbolic runs at a fuel of the block
  count, so a path that loops runs out of fuel: `sym_exec` leaves the path open and the
  `#axioms_within` guard reports `sorryAx`; certify.sh stops. Fails closed. Covering them needs
  a run proof by a simulation at loop heads instead of one bounded run: done for the EVM IR to
  EVM IR stages (cutpoints.py, Counter's `evmir_stages_all`); open for MIR to MIR and MIR to
  EVM IR, which need the exporter to relate a header's values on both sides. Until then
  `--exclude SIG` leaves such a function out of the theorems' scope, which the theorems state.
- **G2** (backend stages, deployment). The stages are about the runtime code; the constructor
  and its initcode are not certified. The theorems hold from any storage, so they hold from
  the constructor's, but that the constructor leaves the invariant (`Counter`'s `limit =
  _limit`) rests on solar-verify alone.
- **G3** (exporter, immutables: `Vault.asset`). `-Zemit-lean-stages` stops with "an immutable
  placeholder of 20 bytes has no Lean semantics". Fails closed.
- **G4** (assembly items, `DUPN`/`SWAPN`/`EXCHANGE`). A `dup` or `swap` past the sixteenth word
  and `exchange` have no item, so a program that uses them fails the walk. Fails closed.
- **G5** (source stage, constructors that write storage: `Counter`'s constructor). The generic
  source stage requires that the constructor leave no storage. Not yet run against these
  examples.
- **G6** (spec to VCs in Lean): solar-verify's own VCs are kernel-checked for Counter, but over
  SolarVerify's embedding, not SolidCore's Loom `wp`; nothing yet shows the two generators ask
  the same of the source, so the spec does not yet meet the source stage in one theorem.
- **B1** (solidity-lean `ck5/token-stage`, built here; `ck5/certified-translation` has the
  same pin and `decode_swap`): `Solar/Assemble.lean` does not build against the solar-tv it pins
  (`2144dccb`), whose `dup` and `swap` positions are `Fin 256` (lines 803 and 809 expect
  `Fin 16`). Everything that imports the generic
  assembler, and so every source and assembly stage, is blocked until it does.

## Notes

- On `alpharush/verify-tv` at `173f2167` none of the committed tv milestones checked: `sym_exec`
  did not reduce the `Fin 256` stack positions of `dup` and `swap` (its stall read as a
  mismatch) and `W1Proofs.Items` did not build. https://github.com/paradigmxyz/solar/pull/2617
  fixes both; this work is stacked on it, and Counter's manifest names its head `c3deb7bf` as the
  compiler.
- The Lean VCs (column 2) are to come from SolidCore's Loom-based generator
  (`SolidCore/Verify` in solidity-lean), checked against solar-verify's obligations. Nothing
  certified may reach `SolidCore/Verify/Finite.lean`, which uses `native_decide`, or close a
  goal with `bv_decide`, which `SolidCore/Verify/Basic.lean` names as one way to close them.
- A `sym_exec` failure from a resource limit (recursion depth, heartbeats) is not a mismatch.
  `negative.sh` reruns such a case with larger limits and requires the mismatch message, so no
  negative here passes by a limit.
