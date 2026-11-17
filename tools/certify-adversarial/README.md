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
