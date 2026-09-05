#!/usr/bin/env python3

"""Contract tests for the manifest-driven cross-server benchmark workflow."""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = ROOT / ".github/workflows/lsp-bench.yml"
COMMENT_WORKFLOW_PATH = ROOT / ".github/workflows/lsp-bench-comment.yml"
MANUAL_COMMAND_WORKFLOW_PATH = (
    ROOT / ".github/workflows/lsp-bench-cross-server-command.yml"
)
CHECKOUT_ACTION = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1"
RUST_ACTION = "dtolnay/rust-toolchain@6c977a6ca4077a0ceb28ffbe03f59d46e9ac8772"
UPLOAD_ACTION = "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
STICKY_COMMENT_ACTION = (
    "marocchino/sticky-pull-request-comment@5770ad5eb8f42dd2c4f34da00c94c5381e49af88"
)
SERVER_ARGS = "--server solar --server asyncswap --server nomic-foundation --server solc"


def workflow() -> str:
    if not WORKFLOW_PATH.is_file():
        raise AssertionError(f"workflow is missing: {WORKFLOW_PATH}")
    return WORKFLOW_PATH.read_text(encoding="utf-8")


def comment_workflow() -> str:
    if not COMMENT_WORKFLOW_PATH.is_file():
        raise AssertionError(f"comment workflow is missing: {COMMENT_WORKFLOW_PATH}")
    return COMMENT_WORKFLOW_PATH.read_text(encoding="utf-8")


def manual_command_workflow() -> str:
    if not MANUAL_COMMAND_WORKFLOW_PATH.is_file():
        raise AssertionError(
            f"manual command workflow is missing: {MANUAL_COMMAND_WORKFLOW_PATH}"
        )
    return MANUAL_COMMAND_WORKFLOW_PATH.read_text(encoding="utf-8")


def job_block(name: str) -> str:
    jobs = workflow().split("\njobs:\n", 1)[1]
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"job {name!r} is missing")
    return match.group(0)


def comment_job_block(name: str) -> str:
    jobs = comment_workflow().split("\njobs:\n", 1)[1]
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"comment job {name!r} is missing")
    return match.group(0)


def manual_command_job_block(name: str) -> str:
    jobs = manual_command_workflow().split("\njobs:\n", 1)[1]
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"manual command job {name!r} is missing")
    return match.group(0)


def step_block(job: str, name: str) -> str:
    marker = f"      - name: {name}\n"
    if marker not in job:
        raise AssertionError(f"step {name!r} is missing")
    remainder = job.split(marker, 1)[1]
    next_step = remainder.find("\n      - ")
    if next_step >= 0:
        remainder = remainder[:next_step]
    return marker + remainder


def github_script(step: str) -> str:
    script = step.split("          script: |\n", 1)[1]
    if not all(
        not line or line.startswith("            ") for line in script.splitlines()
    ):
        raise AssertionError("github-script block has unexpected indentation")
    return "\n".join(line[12:] for line in script.splitlines())


def run_manual_comment_validation(
    provenance: dict[str, object], summary: bytes, *, expect_success: bool
) -> str:
    step = step_block(
        manual_command_job_block("comment"),
        "Validate tested revisions and comment data",
    )
    script = github_script(step)
    harness = f"""
const AsyncFunction = Object.getPrototypeOf(async function () {{}}).constructor;
const workflowScript = {json.dumps(script)};
const outputs = {{}};
const core = {{ setOutput: (name, value) => {{ outputs[name] = value; }} }};
const context = {{
  payload: {{ repository: {{ full_name: "target/solar" }} }},
  runId: 100,
}};
(async () => {{
  const execute = new AsyncFunction("github", "core", "context", "require", workflowScript);
  await execute({{}}, core, context, require);
  process.stdout.write(JSON.stringify(outputs));
}})().catch((error) => {{
  console.error(error.stack || String(error));
  process.exitCode = 1;
}});
"""
    with tempfile.TemporaryDirectory(prefix="cross-lsp-comment-") as directory:
        root = Path(directory)
        summary_path = root / "summary.json"
        provenance_path = root / "provenance.json"
        summary_path.write_bytes(summary)
        provenance_path.write_text(json.dumps(provenance), encoding="utf-8")
        environment = os.environ.copy()
        environment.update(
            {
                "EXPECTED_BASE_REF": "main",
                "EXPECTED_BASE_SHA": "a" * 40,
                "EXPECTED_HEAD_REF": "feature",
                "EXPECTED_HEAD_REPO": "contributor/solar",
                "EXPECTED_HEAD_SHA": "b" * 40,
                "EXPECTED_MERGE_SHA": "c" * 40,
                "EXPECTED_PR_NUMBER": "12",
                "GITHUB_RUN_ATTEMPT": "1",
                "GITHUB_RUN_ID": "100",
                "PROVENANCE_PATH": str(provenance_path),
                "SUMMARY_PATH": str(summary_path),
            }
        )
        completed = subprocess.run(
            ["node"],
            input=harness,
            text=True,
            capture_output=True,
            check=False,
            timeout=10,
            env=environment,
        )
    if expect_success:
        completed.check_returncode()
        return completed.stdout
    if completed.returncode == 0:
        raise AssertionError("comment validation unexpectedly accepted provenance")
    return completed.stderr


def run_workflow_comment_validation(
    provenance: dict[str, object],
    summary: bytes,
    pull: dict[str, object],
    base: dict[str, object],
    merge: dict[str, object],
    *,
    expect_success: bool,
) -> str:
    step = step_block(
        comment_job_block("comment"),
        "Validate tested revisions and comment data",
    )
    script = github_script(step)
    harness = f"""
const AsyncFunction = Object.getPrototypeOf(async function () {{}}).constructor;
const workflowScript = {json.dumps(script)};
const outputs = {{}};
const core = {{ setOutput: (name, value) => {{ outputs[name] = value; }} }};
const context = {{
  payload: {{
    repository: {{ full_name: "target/solar" }},
    workflow_run: {{ id: 100, run_attempt: 1, head_sha: {json.dumps("b" * 40)} }},
  }},
  repo: {{ owner: "target", repo: "solar" }},
}};
const github = {{
  rest: {{
    pulls: {{ get: async () => ({{ data: {json.dumps(pull)} }}) }},
    repos: {{
      getBranch: async () => ({{ data: {json.dumps(base)} }}),
      getCommit: async () => ({{ data: {json.dumps(merge)} }}),
    }},
  }},
}};
(async () => {{
  const execute = new AsyncFunction("github", "core", "context", "require", workflowScript);
  await execute(github, core, context, require);
  process.stdout.write(JSON.stringify(outputs));
}})().catch((error) => {{
  console.error(error.stack || String(error));
  process.exitCode = 1;
}});
"""
    with tempfile.TemporaryDirectory(prefix="cross-lsp-workflow-comment-") as directory:
        root = Path(directory)
        summary_path = root / "summary.json"
        provenance_path = root / "provenance.json"
        summary_path.write_bytes(summary)
        provenance_path.write_text(json.dumps(provenance), encoding="utf-8")
        environment = os.environ.copy()
        environment.update(
            {
                "EXPECTED_HEAD_REF": "feature",
                "EXPECTED_HEAD_REPO": "contributor/solar",
                "EXPECTED_HEAD_SHA": "b" * 40,
                "EXPECTED_PR_NUMBER": "12",
                "PROVENANCE_PATH": str(provenance_path),
                "SUMMARY_PATH": str(summary_path),
            }
        )
        completed = subprocess.run(
            ["node"],
            input=harness,
            text=True,
            capture_output=True,
            check=False,
            timeout=10,
            env=environment,
        )
    if expect_success:
        completed.check_returncode()
        return completed.stdout
    if completed.returncode == 0:
        raise AssertionError("workflow comment validation unexpectedly accepted provenance")
    return completed.stderr


class CrossServerWorkflowTests(unittest.TestCase):
    def test_triggers_permissions_and_runtime_bounds_are_preserved(self) -> None:
        text = workflow()
        header = text.split("\njobs:\n", 1)[0]
        pr = job_block("pr-smoke")
        full = job_block("full")

        self.assertIn("\n  pull_request:\n", header)
        self.assertIn("\n  workflow_dispatch:\n", header)
        self.assertNotIn("pull_request_target", header)
        self.assertNotIn("issue_comment", header)
        self.assertNotIn("schedule:", header)
        self.assertIn("\npermissions: {}\n", header)

        self.assertIn("if: github.event_name == 'pull_request'", pr)
        self.assertIn("continue-on-error: true", pr)
        self.assertIn("runs-on: ubuntu-24.04", pr)
        self.assertIn("contents: read", pr)
        self.assertIn("cancel-in-progress: true", pr)
        self.assertIn(
            "if: github.event_name == 'workflow_dispatch' && github.ref == 'refs/heads/main'",
            full,
        )
        self.assertIn("runs-on: ubuntu-24.04", full)
        self.assertIn("timeout-minutes: 360", full)
        self.assertIn("contents: read", full)
        self.assertIn("cancel-in-progress: false", full)
        self.assertNotIn("continue-on-error: true", full)

    def test_jobs_use_pinned_checkouts_and_release_builds(self) -> None:
        text = workflow()

        self.assertEqual(text.count(f"uses: {CHECKOUT_ACTION}"), 2)
        self.assertEqual(text.count(f"uses: {RUST_ACTION}"), 2)
        self.assertEqual(text.count("persist-credentials: false"), 2)
        self.assertEqual(text.count('toolchain: "1.96"'), 2)
        self.assertEqual(
            text.count("cargo build --locked --release -p solar-lsp-bench"), 2
        )
        self.assertEqual(
            text.count("cargo build --locked --release -p solar-compiler --bin solar"),
            2,
        )
        self.assertNotIn("github-script", text)
        self.assertIn("name: Verify checked out test merge", text)

    def test_pr_smoke_runs_core4_synthetic_with_failure_tolerance(self) -> None:
        pr = job_block("pr-smoke")
        run = step_block(pr, "Run PR smoke comparison")

        self.assertIn(f"prepare --fixture synthetic {SERVER_ARGS}", pr)
        self.assertIn("run \\\n            --profile pr-smoke", run)
        self.assertIn(SERVER_ARGS, run)
        self.assertIn("--allow-failures", run)
        self.assertIn("--solar-binary target/release/solar", run)
        self.assertIn('--solar-revision "$TESTED_MERGE_SHA"', run)
        self.assertIn('TESTED_MERGE_SHA: ${{ github.sha }}', run)
        self.assertNotIn("--require-authoritative", pr)

    def test_pr_smoke_uploads_only_validated_comment_data(self) -> None:
        pr = job_block("pr-smoke")
        self.assertNotIn("issues: write", pr)
        self.assertNotIn("pull-requests: write", pr)
        self.assertNotIn("github.event.pull_request.base.sha", pr)
        build = step_block(pr, "Build candidate and harness")
        verify = step_block(pr, "Verify checked out test merge")
        stage = step_block(pr, "Stage PR smoke comment data")
        upload = step_block(pr, "Upload PR smoke comment data")
        self.assertIn(
            "SOLAR_LSP_BENCH_BUILD_REVISION: ${{ github.sha }}", build
        )
        self.assertIn('test "$(git rev-parse HEAD)" = "$MERGE_SHA"', verify)
        self.assertIn('test "$(git rev-parse HEAD^2)" = "$HEAD_SHA"', verify)
        self.assertIn("summary.json", stage)
        self.assertIn("provenance.json", stage)
        self.assertIn('TESTED_BASE_SHA="$(git rev-parse HEAD^1)"', stage)
        self.assertIn("git rev-parse HEAD^2", stage)
        self.assertIn("base_sha: process.env.TESTED_BASE_SHA", stage)
        self.assertIn(
            'harness_sha256: digest("target/release/solar-lsp-bench")', stage
        )
        self.assertIn("target/lsp-bench/pr-comment/", upload)
        self.assertNotIn(f"uses: {STICKY_COMMENT_ACTION}", pr)

    def test_manual_full_runs_strict_core4_matrix(self) -> None:
        full = job_block("full")
        run = step_block(full, "Run full comparison")

        self.assertIn(f"prepare {SERVER_ARGS}", full)
        self.assertIn(f"doctor {SERVER_ARGS}", full)
        self.assertIn("run \\\n            --profile full", run)
        self.assertIn(SERVER_ARGS, run)
        self.assertNotIn("--allow-failures", run)
        self.assertIn("--solar-binary target/release/solar", run)
        self.assertIn('--solar-revision "$(git rev-parse HEAD)"', run)
        self.assertNotIn("--require-authoritative", full)

    def test_run_publishes_summary_without_validation_or_rerender(self) -> None:
        text = workflow()

        # A run is self-contained: no same-job validation or report command is
        # needed before publishing the summary generated by the runner.
        self.assertNotRegex(text, r"solar-lsp-bench report(?:\s|\\)")

        for job_name, output, retention in (
            ("pr-smoke", "target/lsp-bench/pr-smoke", 30),
            ("full", "target/lsp-bench/full", 90),
        ):
            job = job_block(job_name)
            publish_name = (
                "Publish PR smoke summary"
                if job_name == "pr-smoke"
                else "Publish full summary"
            )
            publish = step_block(job, publish_name)
            self.assertIn(
                f"if: always() && hashFiles('{output}/summary.md') != ''", publish
            )
            self.assertIn(
                f"cat {output}/summary.md >> \"$GITHUB_STEP_SUMMARY\"", publish
            )

            upload_name = (
                "Upload PR smoke evidence"
                if job_name == "pr-smoke"
                else "Upload full evidence"
            )
            upload = step_block(job, upload_name)
            self.assertIn("if: always()", upload)
            for name in (
                "summary.md",
                "summary.json",
                "samples.json",
                "samples.jsonl",
            ):
                self.assertIn(f"{output}/{name}", upload)
            self.assertIn("tools/lsp-bench/benchmark.yaml", upload)
            self.assertIn("tools/lsp-bench/servers.lock.yaml", upload)
            self.assertIn("tools/lsp-bench/fixtures.lock.yaml", upload)
            self.assertIn("tools/lsp-bench/install/", upload)
            self.assertIn("target/lsp-bench/provenance/", upload)
            self.assertIn(f"retention-days: {retention}", upload)
            self.assertIn("if-no-files-found: warn", upload)

        self.assertEqual(text.count(f"uses: {UPLOAD_ACTION}"), 3)
        self.assertIn("${{ github.run_id }}-${{ github.run_attempt }}", text)

        comment_upload = step_block(job_block("pr-smoke"), "Upload PR smoke comment data")
        self.assertIn(
            "name: cross-lsp-pr-comment-${{ github.run_id }}-${{ github.run_attempt }}",
            comment_upload,
        )
        self.assertIn("target/lsp-bench/pr-comment/", comment_upload)
        self.assertIn("retention-days: 30", comment_upload)
        self.assertIn("if-no-files-found: warn", comment_upload)

    def test_fork_commenter_is_workflow_run_scoped_and_safe(self) -> None:
        text = comment_workflow()
        header = text.split("\njobs:\n", 1)[0]
        job = comment_job_block("comment")

        self.assertIn("workflow_run:", header)
        self.assertIn("Benchmark/Cross-server Solidity LSP benchmark", header)
        self.assertIn("types: [completed]", header)
        self.assertIn("permissions: {}", header)
        for condition in (
            "github.event.workflow_run.event == 'pull_request'",
            "github.event.workflow_run.conclusion == 'success'",
        ):
            self.assertIn(condition, job)
        for permission in (
            "actions: read",
            "contents: read",
            "issues: write",
            "pull-requests: write",
        ):
            self.assertIn(permission, job)

        self.assertNotIn("pull_request_target", header)
        self.assertIn(f"uses: {CHECKOUT_ACTION}", text)
        self.assertNotIn("path: target/", text)
        for contract in (
            'const workflowPath = ".github/workflows/lsp-bench.yml"',
            "run.repository?.full_name !== repository",
            'run.path === workflowPath || run.path?.startsWith(`${workflowPath}@`)',
            'run.event !== "pull_request"',
            "provenance.run_id !== Number(run.id)",
            "artifact.workflow_run?.id === run.id",
            'artifact.name === expectedName',
            "artifact-ids: ${{ steps.artifact.outputs.artifact_id }}",
            "run-id: ${{ github.event.workflow_run.id }}",
            "${{ runner.temp }}/cross-lsp-comment",
            'summary="$REPORT_ROOT/summary.json"',
            'provenance="$REPORT_ROOT/provenance.json"',
            'pull.head?.sha !== run.head_sha',
            'pull.head?.repo?.full_name !== headRepository',
            'pull.base?.repo?.full_name !== repository',
            "pull.mergeable !== true",
            "github.rest.repos.getBranch",
            'pull.merge_commit_sha !== provenance.merge_sha',
            'base.commit?.sha !== provenance.base_sha',
            'parents[0] !== provenance.base_sha',
            'parents[1] !== provenance.head_sha',
            'base.commit?.sha !== process.env.BASE_SHA',
            'parents[0] !== process.env.BASE_SHA',
            'parents[1] !== process.env.HEAD_SHA',
            "header: cross-lsp-benchmark",
            "number_force: ${{ steps.pr.outputs.number }}",
        ):
            self.assertIn(contract, text)
        self.assertNotIn("lsp-bench-command", text)

        renderer = step_block(job, "Render comment from validated data")
        self.assertIn("trusted-renderer/tools/lsp-bench/Cargo.toml", renderer)
        self.assertIn("--expected-harness-revision", renderer)
        self.assertIn("--expected-harness-sha256", renderer)
        self.assertIn("--expected-profile pr-smoke", renderer)
        self.assertNotIn("report.md", step_block(job, "Isolate comment data"))

    def test_manual_cross_server_command_is_split_by_permissions(self) -> None:
        text = manual_command_workflow()
        header = text.split("\njobs:\n", 1)[0]
        resolve = manual_command_job_block("resolve")
        benchmark = manual_command_job_block("benchmark")
        comment = manual_command_job_block("comment")

        self.assertIn("issue_comment:", header)
        self.assertIn("types: [created]", header)
        self.assertIn("permissions: {}", header)
        self.assertNotIn("pull_request_target", header)
        self.assertIn("github.event.comment.body == '/bench cross-server'", resolve)
        self.assertNotIn('github.event.comment.body == "/bench lsp"', text)
        for association in (
            '"OWNER"',
            '"MEMBER"',
            '"COLLABORATOR"',
            '"CONTRIBUTOR"',
        ):
            self.assertIn(association, resolve)
        self.assertIn("pull-requests: read", resolve)
        self.assertIn("merge_sha: ${{ steps.resolve.outputs.merge_sha }}", resolve)
        for contract in (
            "!SHA_RE.test(pull.merge_commit_sha || \"\")",
            "github.rest.repos.getCommit",
            "github.rest.repos.getBranch",
            "pull.mergeable !== true",
            "parents.length !== 2",
            '!SHA_RE.test(parents[0] || "")',
            "base.commit?.sha !== parents[0]",
            "parents[1] !== pull.head.sha",
            'core.setOutput("base_sha", base.commit.sha)',
            'core.setOutput("merge_sha", pull.merge_commit_sha)',
        ):
            self.assertIn(contract, resolve)

        self.assertIn("needs: resolve", benchmark)
        self.assertIn("contents: read", benchmark)
        self.assertNotIn("issues: write", benchmark)
        self.assertNotIn("pull-requests: write", benchmark)
        self.assertIn(f"uses: {CHECKOUT_ACTION}", benchmark)
        self.assertIn(
            "ref: refs/pull/${{ needs.resolve.outputs.pr_number }}/merge", benchmark
        )
        self.assertIn("persist-credentials: false", benchmark)
        self.assertIn('test "$(git rev-parse HEAD)" = "$MERGE_SHA"', benchmark)
        self.assertIn('test "$(git rev-parse HEAD^1)" = "$BASE_SHA"', benchmark)
        self.assertIn('test "$(git rev-parse HEAD^2)" = "$HEAD_SHA"', benchmark)
        build = step_block(benchmark, "Build candidate and harness")
        self.assertIn(
            "SOLAR_LSP_BENCH_BUILD_REVISION: ${{ needs.resolve.outputs.merge_sha }}",
            build,
        )
        stage = step_block(benchmark, "Stage comment data")
        self.assertIn(
            'harness_sha256: digest("target/release/solar-lsp-bench")', stage
        )
        self.assertIn("run \\\n            --profile pr-smoke", benchmark)
        self.assertIn(SERVER_ARGS, benchmark)
        self.assertIn("--allow-failures", benchmark)
        self.assertIn("--solar-revision \"$TESTED_MERGE_SHA\"", benchmark)
        self.assertIn("artifact_id: ${{ steps.upload.outputs.artifact-id }}", benchmark)
        self.assertIn(
            "name: cross-lsp-manual-${{ github.run_id }}-${{ github.run_attempt }}",
            benchmark,
        )

        self.assertIn("needs: [resolve, benchmark]", comment)
        self.assertIn("needs.benchmark.result == 'success'", comment)
        self.assertIn("actions: read", comment)
        self.assertIn("issues: write", comment)
        self.assertIn("pull-requests: write", comment)
        self.assertIn(f"uses: {CHECKOUT_ACTION}", comment)
        for contract in (
            "artifact-ids: ${{ needs.benchmark.outputs.artifact_id }}",
            "run-id: ${{ github.run_id }}",
            "${{ runner.temp }}/cross-lsp-manual-comment",
            'summary="$REPORT_ROOT/summary.json"',
            'provenance="$REPORT_ROOT/provenance.json"',
            "Expected regular summary.json and provenance.json at artifact root",
            "Validate tested revisions and comment data",
            "Comment provenance does not match the benchmark request",
            "Summary digest does not match comment provenance",
            "Checkout trusted report renderer",
            "ref: ${{ github.sha }}",
            "path: trusted-renderer",
            "Render comment from validated data",
            "--expected-harness-revision \"$EXPECTED_MERGE_SHA\"",
            "--expected-harness-sha256 \"$EXPECTED_HARNESS_SHA256\"",
            "--expected-profile pr-smoke",
            'pull.merge_commit_sha !== process.env.MERGE_SHA',
            'base.commit?.sha !== process.env.BASE_SHA',
            'parents[0] !== process.env.BASE_SHA',
            'parents[1] !== process.env.HEAD_SHA',
            "header: cross-lsp-command",
            "number_force: ${{ needs.resolve.outputs.pr_number }}",
        ):
            self.assertIn(contract, text)
        self.assertNotIn("find -P \"$REPORT_ROOT\" -type f -name report.md", text)
        self.assertNotIn("cp -- \"$report\"", text)
        self.assertNotIn("cross-lsp-benchmark", text)
        self.assertNotIn("lsp-bench-command", text)

    @unittest.skipUnless(shutil.which("node"), "Node.js is required for workflow tests")
    def test_manual_comment_provenance_is_strict_and_digest_bound(self) -> None:
        summary = b'{"schema_version":7}\n'
        provenance: dict[str, object] = {
            "schema_version": 1,
            "kind": "solar-cross-lsp-comment-data",
            "repository": "target/solar",
            "pr_number": 12,
            "run_id": 100,
            "run_attempt": 1,
            "base_ref": "main",
            "base_sha": "a" * 40,
            "head_repository": "contributor/solar",
            "head_ref": "feature",
            "head_sha": "b" * 40,
            "merge_sha": "c" * 40,
            "harness_sha256": "d" * 64,
            "summary_sha256": hashlib.sha256(summary).hexdigest(),
        }

        outputs = json.loads(
            run_manual_comment_validation(provenance, summary, expect_success=True)
        )
        self.assertEqual(outputs, {"harness_sha256": "d" * 64})

        with_extra = dict(provenance, unexpected=True)
        error = run_manual_comment_validation(
            with_extra, summary, expect_success=False
        )
        self.assertIn("does not match the exact schema", error)

        wrong_revision = dict(provenance, base_sha="e" * 40)
        error = run_manual_comment_validation(
            wrong_revision, summary, expect_success=False
        )
        self.assertIn("does not match the benchmark request", error)

        error = run_manual_comment_validation(
            provenance, summary + b" ", expect_success=False
        )
        self.assertIn("Summary digest does not match", error)

    @unittest.skipUnless(shutil.which("node"), "Node.js is required for workflow tests")
    def test_workflow_comment_accepts_stale_payload_base_and_rejects_stale_merges(
        self,
    ) -> None:
        summary = b'{"schema_version":7}\n'
        provenance: dict[str, object] = {
            "schema_version": 1,
            "kind": "solar-cross-lsp-comment-data",
            "repository": "target/solar",
            "pr_number": 12,
            "run_id": 100,
            "run_attempt": 1,
            "base_ref": "main",
            "base_sha": "a" * 40,
            "head_repository": "contributor/solar",
            "head_ref": "feature",
            "head_sha": "b" * 40,
            "merge_sha": "c" * 40,
            "harness_sha256": "d" * 64,
            "summary_sha256": hashlib.sha256(summary).hexdigest(),
        }
        pull: dict[str, object] = {
            "state": "open",
            "mergeable": True,
            "base": {
                "repo": {"full_name": "target/solar"},
                "ref": "main",
                "sha": "e" * 40,
            },
            "head": {
                "repo": {"full_name": "contributor/solar"},
                "ref": "feature",
                "sha": "b" * 40,
            },
            "merge_commit_sha": "c" * 40,
        }
        base: dict[str, object] = {"commit": {"sha": "a" * 40}}
        merge: dict[str, object] = {
            "sha": "c" * 40,
            "parents": [{"sha": "a" * 40}, {"sha": "b" * 40}],
        }

        outputs = json.loads(
            run_workflow_comment_validation(
                provenance, summary, pull, base, merge, expect_success=True
            )
        )
        self.assertEqual(outputs["base_sha"], "a" * 40)
        self.assertEqual(outputs["merge_sha"], "c" * 40)

        stale_base = {"commit": {"sha": "f" * 40}}
        error = run_workflow_comment_validation(
            provenance, summary, pull, stale_base, merge, expect_success=False
        )
        self.assertIn("does not match the tested merge", error)

        wrong_merge = {
            "sha": "c" * 40,
            "parents": [{"sha": "b" * 40}, {"sha": "a" * 40}],
        }
        error = run_workflow_comment_validation(
            provenance, summary, pull, base, wrong_merge, expect_success=False
        )
        self.assertIn("does not match the tested merge", error)


if __name__ == "__main__":
    unittest.main()
