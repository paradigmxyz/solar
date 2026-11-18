# Fix plan: the seven wrong-stage negatives and falseenv's record

Paper only, from the receipts of the run on solar 65d87ab3 with solidity-lean 334d1d92 (harness
ec92fe40, stages.tsv 23ff1ff7). Nothing here changes stages.tsv, cases.tsv or any expectation:
each case stays not counted and OPEN until a fix lands and the case is rerun against its registered
stage. "Observed" quotes the driver's last line from the case's certify.log.

## Summary

| Case | Registered stage | Observed stage | Why | Fix owner |
|---|---|---|---|---|
| clampwrongspec-r2 | `REJECTED: spec: clamp_meets0 does not check` | `BLOCKED: generic source check … clamp(uint256,uint256,uint256): trees differ` (exit 3) | canonicalizer has no `gt a b = lt b a` | solidity-lean `Tree.canon` |
| siblingfns-r2 | `REJECTED: spec: lo_meets0 does not check` (not naming `hi_`) | `BLOCKED: generic source check … lo(uint256): trees differ` (exit 3) | no `gt x c = not (lt x (c + 1))` with branches swapped | solidity-lean `Tree.canon` |
| initbroken-r2 | `REJECTED: … invariant … (initial\|constructor\|deploy)` | `BLOCKED: generic source check … inc(): trees differ` (exit 3) | checked add: `lt x max` then `lt (x + 1) x` against one `eq x max`; and no invariant stage | solidity-lean `Tree.canon`, then the driver |
| invbroken-r2 | `REJECTED: … invariant … reset` | `BLOCKED: generic source check … inc(): trees differ` (exit 3) | same checked-add spelling; and no invariant stage | solidity-lean `Tree.canon`, then the driver |
| flag-succeedsalways | `REJECTED: spec: lower_succeeds0 does not check` | `REJECTED: scripts/certify: … Final does not check (…Shape): the proof is not of the published statement` (exit 1) | the per-function tree has no callvalue guard, so `lower_succeeds` holds; the false clause fails only in Shape | driver (spec_connect, Shape generator); stage name |
| vacuoussecond-r2a | `REJECTED: … vacuous … two` | same Shape refusal as above (exit 1) | `two`'s premise `y < 3 && y > 5` is unsatisfiable; `Shape.pre_sat_two` fails as a build error, not a named rejection | driver (name the failing `pre_sat_<fn>`) |
| vacuoussecond-r2b | `REJECTED: … vacuous … two` | same Shape refusal (exit 1) | same as r2a | driver |
| falseenv (fail-closed) | `BLOCKED: … a spec clause reads the environment (block.chainid)` | that line, matched (exit 3) | the harness's producer and provenance checks need a build the driver never reached | harness (T5) |

## The canonicalizer cases (4)

Evidence: the probe trees in each case's `rec/record.json` (receipts `alpharush/t5-receipts-c3a` and
`-c3b`). The source and solar trees agree on every input and differ only in how a comparison is
spelled. `Tree.canon` (solidity-lean `SolidCore/Refinement/Generic/Simp.lean`) compares canonical
forms with `=`, and its `.gt` cases only fold impossible comparisons and record bounds.

- clampwrongspec-r2: the source has `ite (gt arg0 arg2) …`, solar `ite (lt arg2 arg0) …`, same sides.
- siblingfns-r2: the source has `ite (gt arg0 5) revert (store …)`, solar `ite (lt arg0 6) (store …) revert`.
- initbroken-r2, invbroken-r2: `inc()`'s `require(x < max); x = x + 1`. The source has the
  require's `lt x max` and then the checked add's own `lt (x + 1) x` overflow test; solar folds both
  into one `eq x max`.

Fix, in solidity-lean, each rule with its soundness lemma (`canon_sound`) so the comparison stays
semantic:

1. `gt a b ↦ lt b a`.
2. `ite (lt b a) t e ↦ ite (not (lt a (b + 1))) …` normalised one way: pick one orientation of a
   strict test against a literal, `gt x c ↔ ¬ lt x (c + 1)` for `c < 2^256 - 1`, and swap branches
   on a negated condition.
3. The checked-add guard: under a dominating `lt x max`, `lt (x + 1) x` is `false`, and the
   remaining `lt x max` against `eq x max` is the negated condition with swapped branches.

Then rerun the four cases unchanged. clampwrongspec-r2 and siblingfns-r2 then reach SpecConnect.
initbroken-r2 and invbroken-r2 also need the second fix: at 65d87ab3 the driver reads no
`@custom:invariant` (generic.py mentions invariants only for loops and `--invariants`). Without an
invariant check (initial state from the constructor, preservation by each function), the two cases
cannot reach their registered stage even after canonicalisation. A pass would then be a finding.

## The shape-stage cases (3)

flag-succeedsalways (receipt `alpharush/t5-receipts-c3`): the log says
`spec connected generically: …raise_holds …lower_holds`, so SpecConnect checked `lower`, its
`succeeds` included. Then `Shape.lean:126:53: unsolved goals` with `hi : 0 + 1 < …specs.length`
(the second function, `lower`), and `#print axioms published` gained `sorryAx`. Reading: the
per-function tree (`lower_tree`) is the body only. A nonpayable function's callvalue revert is in
the dispatcher, so `TreeSucceeds` of `succeeds-if true` holds for the body. The false clause
surfaces only when Shape composes the dispatcher. The registered stage name is also wrong: the
driver's theorem is `<fn>_succeeds` (spec_connect.py line 359), not `<fn>_succeeds0`.

Fix: the driver adds the dispatcher's callvalue guard of a nonpayable entry to the per-function
succeeds obligation (or a separate named check, `<fn>_payable_ok`). Then a `succeeds-if` that admits
`msg.value != 0` on a nonpayable function is rejected as `REJECTED: spec: lower_succeeds …`. The stage
regex for this case then needs the real theorem name. That is a stages.tsv change for coordinator
review, not part of this plan.

vacuoussecond-r2a and -r2b (receipts `-c3b`, `-c3c`): `Shape.lean:181:125: unsolved goals` in case
`refine_1` with the witness call `calldata := Abi.calldata 3475670546 [0]` and goal `⊢ False`. Then
`#print axioms pre_sat_two` gained `sorryAx`. The driver does check per-function satisfiability
(`Shape.pre_sat_<fn>`), and for `two` it fails, as it should: `y < 3 && y > 5` has no witness. But the
failure is a Lean build error that the driver reports as the generic "proof is not of the published
statement".

Fix: the driver decides the premise's satisfiability before it writes Shape (for example a
decided witness search over the premise). It then refuses with
a named reason, such as `REJECTED: vacuous: two: no call satisfies @custom:requires`, which the
registered regex matches as is.

## falseenv: the producer check and the missing build provenance

falseenv's refusal is the registered one: `BLOCKED: environment (1x, first line 12: a spec clause
reads the environment (block.chainid), …)`. The driver blocks before building solar, so its record
has `solar: {commit, dirty}` only: no `binary_sha256` and no `build` block (`record.json` in receipt
`alpharush/t5-receipts-c3c`). producer_check.py then compares `record_binary_sha256: null` with
the producer's digest and reports `mismatch`, and gate.py sees `provenance_version: None`. Both
come from the harness asking a pre-build refusal for build facts it never produced. The driver is
not at fault.

Fix, in the harness (T5's path, a new sha for review before any run):

1. producer_check.py: when the record's status is `BLOCKED` and it has no `solar.binary_sha256`,
   report `not_built` and require `solar.commit` to equal the driver commit and `dirty` false.
   Never report `ok`.
2. gate.py: for class `failclosed` only, accept `not_built` with `solar.commit == driver` in
   place of the provenance-version check, and record that it did. `semantic` and `control` keep
   requiring `provenance_version == 2` and `ok`.
3. Tests: a pre-build BLOCKED record passes the failclosed gate only with the driver commit, and
   the same record fails as a semantic or control case.

falseenv stays recorded as refused as registered. Fail-closed cases are never counted, so the
tally does not change.
