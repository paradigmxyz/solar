# Recursion: cut points at recursive call sites (design)

Status: design only. Today the generic driver refuses recursion as `BLOCKED: recursion` (exit 3)
before any build; see [README.md](README.md#the-generic-drivers-supported-subset).

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
