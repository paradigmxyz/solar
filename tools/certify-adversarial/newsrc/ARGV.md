# New-source cold-replay package (prepared, not run)

## Source

`newsrc/Thermostat.sol`, sha256 `6831c1a3bff4d944598be32524a4e942ea20e13c810163d6819911b9d3e2daea`, contract
`Thermostat`. The driver names its library `thermostat_6831c1a3`. The name appears nowhere in solar
at df9aaa39 or ec92fe40, in any solar branch's tree searched, or in solidity-lean at 334d1d92 or
70db97fa (`git grep -il thermostat`). It uses only shapes the controls certified: parameter
stores, an `if (t > c)` split as in SiblingOk, and `old(..)` frames as in FlagControl. It avoids the
`require` comparison and checked-add spellings that the canonicalizer refuses (FIXPLAN.md). solc
0.8.35 compiles it: `setTarget(uint256)` b480eac0, `off()` 6626b26d, `target()` d4b83992,
`mode()` 295a5212.

## Pins

- solar (driver): `df9aaa394ee663240c537302a4c6937894e15419`
- solidity-lean: `70db97fa7ec395eb0e25ab9abb80e45e6cbe4d68`

**Precondition, not met at these pins.** At df9aaa39, `tools/certify/solidity-lean.rev` is still
`334d1d924f8f6c29ac827fd81d1cc739721b479a`. The driver refuses any other base, in generic.py at
df9aaa39, around line 4631:

    the base {base} is not the pin {pin_rev} of tools/certify/solidity-lean.rev

So the argv below runs on 70db97fa only on a driver head whose `solidity-lean.rev` is 70db97fa. That
pin move belongs to the driver's writer, by coordinator decision. Never use the
`CERTIFY_ALLOW_UNPINNED` hatch.

## Why a changed pin matters (the dependency)

70db97fa descends from 334d1d92 (19 commits). Full-index diff sha256:
`dd7e958d5bfd8329846f8019b96c1c739ea010af5f078f5c5349e4fc9223966e`. It changes the published
statement every generic contract is certified against, `Verify/Generic/Expected.lean` (+48/-16,
diff sha256 `d331a655e16f12f637d887f4228de91c1f16b2415d041e207a1ca33a1652561c`, commit 074be897):

- `structure Source` becomes `structure Source (ctorArgs : List Nat := [])`.
- `Source.semAt` now unfolds to `solidCoreFromArgs ctx.caller src.fuel ws` instead of
  `solidCoreFrom ctx.caller src.fuel`.
- `Expected` and its corollaries become polymorphic in `ws`.

The same commit proves `Source.semAt_nil : src.semAt ctx = solidCoreFrom ctx.caller src.fuel` by
`rfl`. So the statement's meaning for a contract without constructor arguments is unchanged, but
the statement constant and its closure are different terms. It also adds
`SolidCore/Refinement/Generic/{Statement,StageFrom,LoopRun}.lean`. The lakefile change, Counter's
`-M 3000` to `6000`, is resources only.

solar df9aaa39 descends from 65d87ab3 (6 commits, full-index diff sha256
`1e6ac9669aaadeeb34998502630499e63ff0eb4d0b3f7e351fbdb5964503cba5`). Its changes:

- lifecycle tier binding and loop cuts;
- keeping failed lake logs;
- the generated library's lean memory cap (`GEN_LEAN_MEMORY = 12000`).

Reading the diff, I found no change to the certified-tier natspec statement generation; the
default `fuel` stays 4096. I haven't confirmed that by a run.

Conclusion. The c1b and c2 replays pin 334d1d92 and re-check that pin's closure, so they stay
valid as results on 334d1d92. They don't carry over to 70db97fa, because the statement they prove
is a different constant there. A result on 70db97fa needs its own certify and cold replay. The
expected difference is none in outcome, by `semAt_nil`. That is the rationale for this package,
and it holds only once the pin move above exists.

## argv

Box layout as in the earlier phases:

- `~/solar`: a fresh clone at the driver head, empty `target`.
- `~/sl`: solidity-lean at the pin.
- `~/pk`: a lake-fetched clone at the same pin.
- `~/harness`: this branch.

CERTIFY_ALLOW_* unset.

Certify (its own box and its own memory scope, as c1):

    cd ~/solar && tools/certify/certify ~/harness/tools/certify-adversarial/newsrc/Thermostat.sol natspec \
      --solidity-lean ~/sl --base 70db97fa7ec395eb0e25ab9abb80e45e6cbe4d68 \
      --work ~/run/thermostat/work --record-dir ~/run/thermostat/rec --packages-from ~/pk

Expected: exit 0, `CERTIFIED: thermostat_6831c1a3: the requested tier holds (…)`, tier certified,
replay closure fully replayed, trusted 0, axioms exactly propext, Classical.choice and Quot.sound,
`solar.build.provenance_version` 2, and `binary_commit` equal to the driver head.

Cold replay (another box, a fresh `~/pk`, from the certify archive checked by sha256):

    cd ~/solar && tools/certify/certify --replay ~/run/thermostat/rec \
      --solidity-lean ~/sl --packages-from ~/pk

There is no `--fresh` flag at df9aaa39. `--replay DIR` is the fresh cold run ("check a finished run's
directory against a fresh cold run"). Pass: exit 0 and the last line ends
`: a fresh cold run from the same pinned source/build inputs gave identical checked outputs (…)`,
with a fresh record meeting the same conditions as above. c2.sh already checks exactly this.

Thermostat isn't in cases.tsv or stages.tsv, which are frozen. It runs by this argv alone, unless
the coordinator admits a new row.

## The negatives' registered stages (stages.tsv 23ff1ff7)

| Case | Class | Registered stage (regex on the driver's last line) | Why that stage |
|---|---|---|---|
| siblingleak | semantic | `^REJECTED: spec: \S*\bset_meets0 does not check` | `x <= 5 ==> level == 1` is false on the else branch (level is 2) |
| flag-specwrong | semantic | `… raise_meets0 does not check` | `raise` claims `value == v + 1` and stores `v` |
| flag-branchleak | semantic | `… raise_meets0 does not check` | `raise` stores 8 when `v == 7` only |
| flag-frameleak | semantic | `… lower_meets0 does not check` | `lower` also clears `value`; the spec keeps `value == old(value)` |
| flag-storagealias | semantic | `… lower_meets0 does not check` | `lower` copies slot 1 (`raised`) into slot 0 (`value`) |
| flag-succeedsalways | semantic | `… lower_succeeds0 does not check` | `succeeds-if true` on a nonpayable function |
| meterwrongspec | semantic | `… record_meets0 does not check` | `record` claims `total == old(total) + v + 1` |
| clampwrongspec-r2 | semantic | `… clamp_meets0 does not check` | ensures `result > a` is false at `x < a` |
| siblingfns-r2 | semantic | `… lo_meets0 does not check` (not `hi_`) | only `lo`'s stored value is wrong |
| initbroken-r2 | semantic | `^REJECTED: .*\binvariant\b.*\b(initial\|constructor\|deploy)` | `x >= 1` fails at deployment (`x` starts at 0) |
| invbroken-r2 | semantic | `^REJECTED: .*\binvariant\b.*\breset\b` | `reset` breaks `x >= 1` |
| vacuoussecond-r2a | semantic | `^REJECTED: .*\bvacuous.*\btwo\b` (either order) | `two`'s requires `y < 3 && y > 5` is unsatisfiable |
| vacuoussecond-r2b | semantic | same | same, `succeeds-if` on both functions |
| flag-wrap | failclosed | `^BLOCKED: .*\braise\b.*source tree undefined \(erasure\)` | `raise` stores `v + 1` unchecked and claims `value > v` (wraps at 2^256 - 1) |
| falseenv | failclosed | `^BLOCKED: .*a spec clause reads the environment \(block\.chainid\)` | a spec clause reads the environment the statement fixes |
