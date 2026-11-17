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
