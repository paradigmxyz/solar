# certify-adversarial

Negative and control cases for `tools/certify/certify` at the certified tier, and the harness that
runs them on a short-lived machine. See [CASES.md](CASES.md) for each case's mutation, target stage,
expected exit code and why it is not vacuous.

```bash
SOLAR_DIR=~/solar SOLAR_SHA=<commit> LEAN_DIR=~/solidity-lean LEAN_SHA=<commit> \
BOX_CREATED=<the machine's creation time, ISO 8601> CASES=cases.tsv OUT=~/adv \
SHIP_REMOTE=<git remote> SHIP_BRANCH=<branch> bash tools/certify-adversarial/run.sh
```

- `producer.sh` builds solar the way the driver does and records the binary's path, sha256 and
  embedded commit, the driver and harness commits, the build command and the driver's codegen
  options; it refuses a dirty checkout, another commit, or any build or binary override, including
  either escape hatch. No stage cache is used.
- `run.sh` admits cases until 1h40m after `BOX_CREATED` (and only if a run of `EST_MIN` fits),
  kills every run at 1h55m, and writes `table.md`.
- `case.sh` refuses a driver checkout that is not a clean `SOLAR_SHA`, unsets the escape hatches
  (`CERTIFY_ALLOW_UNPINNED`, `CERTIFY_ALLOW_FOREIGN_SOLAR`) and records them in its status with the
  driver and harness commits, runs one case in its own session under `receipt.py` (memory trace),
  checks the record against the producer (`producer_check.py`), and appends
  `KIND exit=N expect=E match=yes|no producer=… log_sha256=…` to its status.
- `SOLAR_DIR` is the driver checkout at the commit under test; the harness may run from another
  checkout, whose commit is recorded as `harness`.
- `ship.sh` packs each case as it finishes (without its work tree, with a sha256 manifest) and
  pushes it to `SHIP_BRANCH` under `runs/<solar commit>/<case>`.

## On a box

`box/setup.sh SOLAR_SHA HARNESS_SHA LEAN_SHA` prepares a fresh box from public sources (pinned
solc by sha256) and writes every tool version to `~/t5-versions.txt`. `box/go.sh MEMORY_MAX_BYTES`
starts `run.sh` in a transient systemd scope with that `memory.max` and `memory.swap.max` 0, and
starts nothing unless the driver's `tools/certify/box_admit.sh admit` reads both back
(`$OUT.admission`). Each case also re-reads its own cgroup's limits and refuses (`REFUSED-SCOPE`)
when `memory.max` is not a byte count or swap is allowed, and records `memory.peak` and
`memory.events` in `scope.txt`.

## Gate (rerun)

`stages.tsv` registers each case's class and intended stage, `gate.py` checks a finished case against
it (`gate.json`; a control also needs its record's tier, strict predicate and full replay with
trusted 0, plus an independent `--replay` that `case.sh` runs and records in `replay.exit`), and
`table.py` prints the control gate and the semantic rejections that count. See CASES.md, Rerun
criteria.

Control-gate tests: `python3 test_table.py` (the gate over the registered controls: a missing,
ungated or failing control fails it; all three passing passes).

Run-level tests: `bash test_ns.sh test_run.sh "$PWD"` (fake driver and cargo: controls first, replay
arguments, stop on a failing control, producer commit check).

Controls across phases: `join_controls.py C1_DIR C1_SHAS C2_DIR C2_SHAS OUT` joins each control's c1
archive with its c2 replay (both checked against their digest lists; c2 must have replayed the same
record) and reruns `gate.py`; `run.sh` does this before the table when `JOIN_C1_DIR`, `JOIN_C1_SHAS`,
`JOIN_C2_DIR` and `JOIN_C2_SHAS` are set (phase c3). Tests: `python3 test_join.py`.
