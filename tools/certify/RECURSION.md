# Recursion: cut points at recursive call sites (design)

Status: the backend stages and the assembly stage are implemented for one self-recursive function
(`rec_cuts.py`, Phase 3 slices 1 and 2): `cutpoints.stage_body` hands a module whose first MIR stage
recurses to it, and certify.py writes kernel-checked stage proofs from the first lowered MIR to the
final EVM IR and the bytecode (`testdata/rec_cuts/RecSum.sol`; solidity-lean `Verify/Phase3/RecSum`,
whose `Bytecode.lean` proves `total(n)` returns `n * (n + 1) / 2` on the exact bytecode). The assembly
stage is solidity-lean's `AssembleRec.checkF` on a certificate `asm_cert.infer_frames` gives: the
function's cut points are heads whose entry heights come from the last stage's relation (`heads_ok`,
by `reach_cut`), and the stack below the function's blocks is a stack of frames, one per call, each the
words its call site keeps below the callee with the site's return label above them (`FrB`, one
constructor per kind of call site, as the stage relations' `Fr`), so a return types the caller's
words by the label it returns to. The source stage (slice 4) is solidity-lean's `Verify/Phase3/RecSource`: `sum_runs` proves in
SolidCore's own interpreter that the recursive function's body returns its value from every entry at
measure `n`, by induction on `n`, the recursive call running by the theorem at `n - 1` through
SolidCore's internal-call semantics; `total_runs` is the caller's `BodyRuns`, so the generic
dispatcher theorem `runtime_run_clean` gives the source's run of every call; `source_to_bytecode`
joins it to `total_returns_bytecode` (the same output from some gas on, on the bytes
`runtime_is_fresh` binds to this run's compile). `rec_source.py` binds its AST module to the
source file. solar-verify proves the recursion by its measure too (a recursive call must give
`@custom:decreases` a value at least 0 and below the entry's, then uses the contract at the lower
measure), and infers a contract for a recursive function written without one from its callers'
specs, kept only where that proof holds. Still refused: the published shapes (they need the
assembly checker's backward simulation and an explicit gas bound, which AssembleRec does not give
yet), the generic driver's final theorem (shape_gen
finds no plain EVM IR stage chain), and, in the assembly stage, a recursive function whose blocks
touch memory or the environment, call another function, or are entered other than by a call, a
return or a jump within the same frame; see [README.md](README.md#the-generic-drivers-supported-subset).

What rec_cuts.py does, against the proposal below: cut points at the function's entry and at
each recursive call site's resume point; an inductive frame relation `Fr` over the whole call
stack (one constructor per call site, a recursive site's over the frames below), instead of a
summary at the stage level; and, on every EVM IR side, the stack below a frame bounded by the
measure (the argument the recursive calls keep below them, times the words a frame keeps, at
most 984 words), so that no segment can overflow the 1024-word stack. The measure's range comes
from the outer caller's own check (`require(n <= 64)`), which the first segment proves; a
caller with no bound fails that proof. The function's contract itself is proved on the lowered
MIR by induction on the measure (`Verify/Phase3/RecSum/Spec.lean`, `sum_returns`) and carried to
the final EVM IR by the stage chain (`EvmIR.lean`).

## Problem

The stage proofs relate consecutive backend stages by symbolic execution between cut points. A
loop gets its cut point from a kernel-checked invariant certificate; a call to a function whose
body the proof is still executing gets none. For a recursive function, such as
`decrement(n)` calling `decrement(n - 1)`, symbolic execution of the body re-enters the body and
never reaches a cut point, so stage proof generation fails at the first stage pair (the
first lowered MIR's place reads) instead of producing a proof.

## Proposal

1. **A measure in the source.** Each function in a recursive cycle (a strongly connected
   component of the call graph, as the driver computes it now) carries
   `/// @custom:decreases <expr>`, a natural-number expression over its parameters and storage
   it does not write before the call. Mutual recursion shares one measure across the cycle,
   compared lexicographically when it is a tuple. The driver keeps refusing a cycle with a
   member that has none, and keeps refusing calls through internal function values.

2. **A cut point at each recursive call site and at each cycle member's entry.** The entry cut
   point states the stage relation for one call of the function (arguments, the storage and
   memory it reads, its results and writes) as a summary. At a recursive call site the proof
   does not execute the callee: it checks that the measure decreased, and then uses the summary
   for the smaller measure. The summary for every measure follows by well-founded induction on
   the measure (`WellFounded.induction` on `Nat.lt`, or `Prod.Lex` for a tuple), proved once
   per cycle, by the kernel, with the same axioms as today.

3. **The EVM IR stages.** After calls are lowered, a recursive call is a jump to the function's
   entry with a return label and the frame on the stack. The entry cut point there quantifies
   over the stack below the frame (the caller's words, untouched) and the return label, so one
   proof covers every depth. Each frame takes a static number of stack words, so depth is bounded
   by the EVM's 1024-word stack: a call deep enough to overflow halts exceptionally, and the
   statement must either prove that the measure's range keeps the depth under the limit or list
   that halt among the allowed statuses. The same applies to the gas bound: the resources status
   needs a bound linear in the measure, or the run is recorded below the strongest status.

4. **The source stage and the spec.** The erased tree of a recursive function is not finite. The
   source check and `spec_connect` prove the function's spec by the same induction, with the
   spec itself as the summary at the recursive call, rather than through `tree_meets`.

## Limits and open questions

- Only measures over parameters (and storage not written before the call) are proposed; a
  measure over memory or return data is out of scope.
- Whether the inliner or tail-call shapes in MIR can remove the call before the stage pair it
  fails at today, which would move the cut point rather than remove the need for it.
- How the stack-depth bound is stated: as a proved premise of the published statement, or as an
  allowed exceptional halt. Either must be visible in the record's statuses, never assumed.
- The negative cases the proposal needs: a measure that does not decrease, a missing measure, a
  measure that depends on storage the body writes first, and a cycle through a modifier.
