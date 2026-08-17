# Linux benchmark capability probe

This records the capability checks implemented by
`.github/scripts/lsp-bench-probe.sh` and
`.github/workflows/lsp-bench-probe.yml`. The checked-in standalone probe and
full-comparison workflows use GitHub-hosted `ubuntu-24.04`; the full comparison
repeats the probe before preparation so its record and benchmark describe the
same job. The probe is allowed to fail in those portable workflows, and their
`default` profile remains non-authoritative regardless of the probe outcome.

The same script is also the preflight for an operator-run strict `publish`
profile. In that path every check below must pass in the benchmark job itself,
and the operator must ensure the environment is fixed, dedicated, and has no
concurrent jobs. The repository does not currently configure that environment
as an Actions runner.

## Workflow shape

```yaml
name: LSP benchmark environment probe
on:
  workflow_dispatch:
permissions: {}
jobs:
  probe:
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@<pinned-sha>
        with:
          fetch-depth: 0
      - name: Probe optional Linux isolation and accounting
        continue-on-error: true
        shell: bash
        run: .github/scripts/lsp-bench-probe.sh
      - name: Upload probe record
        if: always()
        uses: actions/upload-artifact@<pinned-sha>
        with:
          name: lsp-bench-environment-probe-${{ github.run_id }}-${{ github.run_attempt }}
          path: target/lsp-bench/probe/
          if-no-files-found: error
```

The workflow uses the exact pinned action SHAs already used by the repository.
The standalone workflow reports the capabilities of one ephemeral hosted job. A
full comparison repeats the same script in its benchmark job and retains that
output with the comparison; a probe from a separate workflow run cannot
establish the state of another machine or job.

## Shell checks

The script uses `set -Eeuo pipefail`, writes command output and errno to a
timestamped record, and returns failure if any authority assertion fails. The
checked-in hosted workflows retain that failure as probe evidence and continue
with a portable comparison; a strict publication must stop.

1. Record `uname -a`, `uname -m`, `/etc/os-release`, `findmnt /sys/fs/cgroup`,
   `lscpu`, and `RUNNER_*` metadata. Require Linux x86_64 for the current
   publication standard.

2. Discover the caller's cgroup-v2 path from the `0::PATH` line in
   `/proc/self/cgroup`. Under that parent, create a uniquely named child.
   Require readable `cpu.stat`, `memory.peak`, and `cgroup.events`, and
   writable `cgroup.procs`. Keep the probe's parent shell in the parent cgroup;
   fork a short-lived helper, write the helper PID to the child
   `cgroup.procs`, and wait for it. (Writing `0` would move the writing shell
   itself and would prevent removing the child until it is moved back.) Read
   `cpu.stat` (`user_usec`, `system_usec`), `memory.peak`, and `cgroup.events`
   (`populated 0` after the helper exits), then remove the child. Record every
   failed operation and errno. A readable cgroup mount without successful child
   creation, migration, descendant accounting, and cleanup is a failure.

3. Run one command entirely inside a fresh namespace:
   `unshare --user --map-root-user --net -- bash -c '...'`. Inside it,
   require `lo` to be the only network device, execute `ip link set lo up`, bind a TCP listener to
   `127.0.0.1:0`, connect a client to that listener, exchange a marker, and
   exit. The listener and client must both be children of the same namespace;
   a host-side connection does not prove loopback isolation. Require
   `unshare` exit status zero.

4. Emit a machine-readable `status.env` and upload the complete probe
   directory even on failure. The full-comparison job repeats this probe before
   preparation and includes the directory in the comparison artifact. A failed
   check prevents strict authority but does not block the portable workflow.

The checked-in script uses short Python helpers for the local TCP exchange and
cgroup descendant workload. The helper allocates enough work to make nonzero
CPU and memory deltas observable without imposing a benchmark load.

## Authority wording

Suggested README/report text for a future operator-run strict publication:

> A run is authoritative only if it is x86_64 Linux, records the pinned image
> label plus `uname`, CPU, and runner metadata, passes the delegated cgroup-v2
> child-accounting probe (including forked descendants and
> `cpu.stat`/`memory.peak`/`cgroup.events`), passes unprivileged user+network
> namespace and namespace-local loopback TCP, runs serially on one ephemeral job,
> and every setup/measured process reports CgroupV2 process-tree and total-memory
> metrics. Any fallback to direct-child `rusage`, missing namespace isolation,
> or a failed probe makes the output non-authoritative while retaining raw
> samples.

> The probe records CPU, kernel, and runner metadata for one job but cannot
> prove that a later run uses the same machine or load conditions. Review that
> provenance across publications and maintain the dedicated runner configuration
> outside this repository's portable workflow.

## Why the historical per-server network wrapper is insufficient

The historical harness used
`unshare --user --map-root-user --net -- <server>` but connected to TCP from
the parent process. A network namespace has its own network devices and
sockets; the parent cannot reach a listener bound only in the child namespace.
Also, a new namespace's loopback must be brought up. The full run must therefore
enter one namespace before starting the harness and servers (and run the TCP
client there), or use stdio for servers such as Wake.

## Sources

- Linux cgroup v2 delegation and accounting: <https://docs.kernel.org/admin-guide/cgroup-v2.html>
- Linux `unshare(1)` namespace options: <https://man7.org/linux/man-pages/man1/unshare.1.html>
- Linux network namespace isolation: <https://man7.org/linux/man-pages/man7/network_namespaces.7.html>
- Bringing a namespace-local loopback device up: <https://man7.org/linux/man-pages/man8/ip-netns.8.html>
