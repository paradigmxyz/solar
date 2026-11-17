# Adversarial cases for the certified tier

Each case is a full `tools/certify/certify SOURCE natspec` run in a fresh work directory, with no
stage cache, against one recorded solar binary (`producer.sh`). A negative counts only if it is
rejected at its **target stage**; a rejection anywhere else (the witness check, the envelope, a
build or resource failure) is recorded with its stage and is not evidence for the property. A
CERTIFIED negative is a soundness finding. Resource limits, timeouts and kills (`KILLED-DEADLINE`)
are never a verdict.

Stages, in the driver's order: envelope (source check, exit 3) → witness (`no function has
@custom:succeeds-if`, exit 1) → build solar → export/statement → per-function obligations
(`<f>_succeeds0`, `<f>_meets0`, `<f>_holds`) → replay (trusted=0) and axioms → record.

Status of earlier runs: on solar b35dfeb2 the base cannot certify (missing `cutpoints` module);
those runs are not rejections. On 536759b59 / solidity-lean 98763929 the runs in the last column
were observed but not archived, or aborted when the run box expired, and count for nothing until
rerun with this harness.

| case | source | mutation | target stage (theorem) | exit | why it is not vacuous | earlier run |
|---|---|---|---|---|---|---|
| control-flag | FlagControl.sol | none (held-out positive) | all; must reach CERTIFIED | 0 | `raise`/`lower` succeed whenever msg.value == 0, so the witness is a real success | ABORTED |
| control-siblingok | SiblingOk.sol | none; control of siblingleak, true postcondition for both branches | all; CERTIFIED | 0 | both branches are reachable (x > 5 and x <= 5) | ABORTED |
| siblingleak | SiblingLeak.sol (7e2f4c33) | `x <= 5 ==> level == 1` is false on the else-branch (level is 2); provable only if the then-branch's x > 5 leaks into the sibling | spec: `set_meets0` | 1 | x = 0 succeeds and violates the claim; the premise x <= 5 is satisfiable | unarchived (REJECTED at `set_meets0`) |
| flag-specwrong | FlagSpecWrong.sol | `raise` claims `value == v + 1`; stores v | spec: `raise_meets0` | 1 | every successful call refutes it | unarchived (REJECTED, spec) |
| flag-branchleak | FlagBranchLeak.sol | `raise` stores 8 when v == 7 only | spec: `raise_meets0` | 1 | v = 7 succeeds and refutes `value == v`; one input among 2^256 | unarchived (REJECTED, spec) |
| flag-frameleak | FlagFrameLeak.sol | `lower` also clears value; spec keeps `value == old(value)` | spec: `lower_meets0` (frame) | 1 | any state with value != 0 refutes it | unarchived (REJECTED, spec) |
| flag-storagealias | FlagStorageAlias.sol | `lower` copies slot 1 (raised) into slot 0 (value) | spec: `lower_meets0` (frame across slots) | 1 | any state with value != raised refutes it | unarchived (REJECTED, spec) |
| flag-succeedsalways | FlagSucceedsAlways.sol | `lower` claims `succeeds-if true`; a nonpayable call with value reverts | succeeds: `lower_succeeds0` | 1 | a call with msg.value = 1 reverts. **Caveat:** if the statement fixes callvalue at 0 (ENVIRONMENT: the transaction of Target.message is fixed), the claim is true in the stated scope and exit 0 is a statement-scope gap (calls with value not covered), not kernel unsoundness: record it as such | ABORTED (first try: env failure) |
| meterwrongspec | MeterWrongSpec.sol | `record` claims `total == old(total) + v + 1` | spec: `record_meets0` | 1 | every successful call refutes it. Confound: `succeeds-if msg.value == 0` is also false when total + v overflows (checked add reverts), so a rejection at `record_succeeds0` is a valid fail-closed result but not the targeted one | unarchived (REJECTED, spec) |
| clampwrongspec-r2 | ClampWrongSpecR2.sol | `clamp` claims `result > a`; returns a when x <= a | spec: `clamp_meets0` | 1 | a <= b is satisfiable and x = a refutes it. r2 adds only `succeeds-if` (the original stopped at the witness check) | ABORTED |
| siblingfns-r2 | SiblingFnsR2.sol | `lo` claims `y == 1`, stores 2; its sibling `hi` makes the same claim truly | spec: `lo_meets0` (not `hi`) | 1 | x = 0 succeeds in lo and refutes it; hi must still check, so the rejection must name lo | ABORTED |
| initbroken-r2 | InitBrokenR2.sol | `@custom:invariant x >= 1`, but x starts at 0 | invariant initial check | 1 or 3 | the deployed state x = 0 violates it. **Exit 0 is a finding**: an invariant annotation silently ignored | ABORTED |
| invbroken-r2 | InvBrokenR2.sol | `reset` sets x = 0, breaking `x >= 1` | invariant preservation at `reset`'s exit | 1 or 3 | reset succeeds from any state and leaves x = 0. Exit 3 for the constructor or the invariant is fail-closed but untargeted. **Exit 0 is a finding** | ABORTED |
| vacuoussecond-r2a | VacuousSecondR2a.sol | `two`'s premise `y < 3 && y > 5` is unsatisfiable, so its false claim `x == 9` holds vacuously; only `one` has a witness | per-function non-vacuity of `two` | 1 | the case is the vacuity: the contract-level witness (one) is real, so it tests whether vacuity is checked per function. **Exit 0 is a finding** (a vacuous postcondition certified) | ABORTED |
| vacuoussecond-r2b | VacuousSecondR2b.sol | same, with `succeeds-if` on both functions | per-function non-vacuity of `two` | 1 | as r2a; two succeeds for every y, so its succeeds-if is true and only the requires is vacuous. **Exit 0 is a finding** | ABORTED |
| control-flagrenamed | FlagRenamed.sol | none: FlagControl alpha-renamed (contract Pennant, every identifier), name not in the base lakefile | all; must reach CERTIFIED with an independent replay | 0 | the positive for a new contract through the generic path | new |
| flag-wrap | FlagWrap.sol | `raise` stores `v + 1` unchecked and claims `value > v`; wraps to 0 at v = 2^256 - 1 | spec: `raise_meets0` | 3 now; 1 once unchecked arithmetic is in the fragment | v = 2^256 - 1 refutes it. On 536759b59 it was BLOCKED (`source tree undefined (erasure)`): unchecked arithmetic has no source semantics yet. That is an unsupported-feature gap that fails closed correctly; it is not evidence about wrapping | unarchived (BLOCKED exit 3) |
| falseenv | FalseEnv.sol | premise `block.chainid == 1 && block.chainid == 2`, false claim | envelope: a spec clause that reads the environment, which the statement fixes | 3 | not a semantic negative: the expected outcome is the envelope's refusal (expected fail-closed). It has no succeeds-if, so if the envelope ever admits it, the witness check stops it | b35dfeb2 only (BLOCKED exit 3) |

Expected exit codes: 0 CERTIFIED, 1 REJECTED, 3 BLOCKED. "1 or 3" means either fails closed, and
only 1 at the target stage is evidence for the property.

## Checker-pipeline cases (queued, fault injection)

These take a certified control's real run (control-flag) and change one input of the checking steps
(`tools/certify/replay.py`, `check_final.lean`, the guarded build), then rerun that step. Each must be
REJECTED; any accepted one is a driver finding.

| case | injection | must report |
|---|---|---|
| chk-missing | the checker executable (leanchecker / lean) removed from PATH and the toolchain | REJECTED, checker unavailable (not a skip) |
| chk-empty | the checker's output replaced by an empty file | REJECTED, no accounting |
| chk-truncated | the checker's output cut after half its lines | REJECTED, missing obligations |
| chk-zero | the obligation inventory kept, the checked list emptied | REJECTED, 0 of N checked |
| chk-omitted | one required theorem (`raise_meets0`) and one function dropped from the checked list | REJECTED, names the missing ones |
| chk-stale-twin | the proof twin from another source (SiblingOk's) under this run's name | REJECTED, statement or source hash mismatch |
| chk-killed | the checker killed (SIGKILL) mid-run, and separately one timed out | REJECTED or resource failure, never a verdict of CERTIFIED |
| chk-leaked | a proof block that adds a hypothesis (sibling or branch premise) to an obligation's context | REJECTED, statement differs |
| chk-identity-op | an opcode outside the fragment mapped to identity in the exported stages | REJECTED or BLOCKED, never CERTIFIED |

## Rerun criteria

Registered before the rerun, in `stages.tsv` (NAME, CLASS, STAGE). `run.sh` refuses to start a case
without a row and logs the file's sha256; `gate.py` writes each case's `gate.json`; `table.py`
prints the gate under the table.

1. **Controls first.** control-flag, control-siblingok and control-flagrenamed are class
   `control`. Each must print `CERTIFIED: … the requested tier holds`, and its record must show
   `tier_reached` equal to the requested tier, `strict.holds`, and `replay` with every closure module
   replayed, `trusted_count` 0 and nothing unaccounted. Its independent replay
   (`tools/certify/certify --replay` of its record, within the same deadline) must then exit 0. If
   any control fails, no negative counts ("Control gate: FAIL").
2. **Stage-bound negatives.** A `semantic` case counts only when its run ended (not killed) and
   the driver's verdict line matches its registered STAGE: for the spec cases, the named theorem
   (`REJECTED: spec: …<fn>_meets0 does not check`, or `<fn>_succeeds0`). Exit 1 from elsewhere
   (`REJECTED: resources:`, `REJECTED: scripts/certify: exits …`, an input-commit refusal, a driver
   error) and exit 3 (an unsupported feature or a tree mismatch) are not semantic rejections. A
   timeout, a deadline kill or NOT-ADMITTED is no verdict. `siblingfns-r2` also must not name
   `hi_`.
3. **Fail-closed cases are not evidence.** flag-wrap and falseenv are class `failclosed`: their
   registered refusal is recorded, never counted.
4. **Renamed positive.** control-flagrenamed (`cases/FlagRenamed.sol`, contract `Pennant`) is
   FlagControl with every identifier renamed and nothing else changed. Its name is in no base
   lakefile, so it certifies only if the generic path adds a new contract on its own: the path the
   afaefd40 input-commit refusal broke.

Expected gaps at the driver as of afaefd40: tools/certify has no `@custom:invariant` stage and only a
contract-level non-vacuity witness. initbroken-r2, invbroken-r2 and vacuoussecond-r2a/b are
registered at the stage the property needs (an invariant check naming the initial state or
`reset`, a per-function vacuity check naming `two`). A driver without that stage cannot meet it,
so those cases cannot count there. Exit 0 for any of them stays a finding.

## Why clampwrongspec-r2 and siblingfns-r2 were BLOCKED on afaefd40

Diagnosis from the receipts' records, without a box or a build. Both are a gap in the probe's tree canonicalizer, not the
mutation. The source and solar trees below are equal on every input; they differ only in how a
comparison is spelled. `Tree.canon` (solidity-lean `SolidCore/Refinement/Generic/Simp.lean` at
334d1d92) compares canonical forms by `=`. It has no rule turning `gt a b` into `lt b a`, or
`gt a c` into `lt a (c + 1)` with the sides swapped. Its `.gt` cases only fold a comparison
that cannot hold, and record bounds. So the probe answers "trees differ" and the driver blocks
(exit 3, fail-closed, sound).

- clampwrongspec-r2: `clamp`'s second test is `x > b`. Source: `ite (gt arg0 arg2) (halt true [arg2]) (halt true [arg0])`.
  solar: `ite (lt arg2 arg0) …` with the same sides. The mutation is spec-only
  (`result > a` is false at `x < a`), so the code trees should be equal, and they are up to
  `gt a b = lt b a`.
- siblingfns-r2: `lo`'s `require(x <= 5)`. Source: `ite (gt arg0 5) (revert) (store slot0 := 2)`.
  solar: `ite (lt arg0 6) (store slot0 := 2) (revert)`. The mutated value (2, not 1) is identical
  on both sides, and the trees are equal up to `gt x 5 = not (lt x 6)`. `hi` (`x > 5`) passed.
- initbroken-r2, for comparison, blocked on `inc`'s checked add. Source:
  `lt x max` followed by `lt (x + 1) x`. solar: one `eq x max`. It is also a canon gap; there
  exit 3 is within its expectation (1|3), but it is not evidence.

The two NO results stand. The cases test their target stage only once the canonicalizer covers
these spellings, or with case variants that avoid them (new names, not an edit of the r2 cases).
