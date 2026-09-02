#!/usr/bin/env python3

"""Contract tests for the manifest-driven cross-server benchmark workflow."""

from __future__ import annotations

import re
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

    def test_pr_smoke_runs_core4_synthetic_with_failure_tolerance(self) -> None:
        pr = job_block("pr-smoke")
        run = step_block(pr, "Run PR smoke comparison")

        self.assertIn(f"prepare --fixture synthetic {SERVER_ARGS}", pr)
        self.assertIn("run \\\n            --profile pr-smoke", run)
        self.assertIn(SERVER_ARGS, run)
        self.assertIn("--allow-failures", run)
        self.assertIn("--solar-binary target/release/solar", run)
        self.assertIn('--solar-revision "$(git rev-parse HEAD)"', run)
        self.assertNotIn("--require-authoritative", pr)

    def test_same_repository_pr_publishes_a_distinct_sticky_comment(self) -> None:
        pr = job_block("pr-smoke")
        comment = step_block(pr, "Publish PR smoke comment")

        self.assertIn("issues: write", pr)
        self.assertIn("pull-requests: write", pr)
        self.assertIn(f"uses: {STICKY_COMMENT_ACTION}", comment)
        self.assertIn(
            "github.event.pull_request.head.repo.full_name == github.repository", comment
        )
        self.assertIn(
            "hashFiles('target/lsp-bench/pr-smoke/summary.md') != ''", comment
        )
        self.assertIn("header: cross-lsp-benchmark", comment)
        self.assertIn("path: target/lsp-bench/pr-smoke/summary.md", comment)
        self.assertNotIn("lsp-bench-command", comment)

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

        comment_upload = step_block(
            job_block("pr-smoke"), "Upload PR smoke comment report"
        )
        self.assertIn(
            "name: cross-lsp-pr-comment-${{ github.run_id }}-${{ github.run_attempt }}",
            comment_upload,
        )
        self.assertIn("target/lsp-bench/pr-comment/report.md", comment_upload)
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
            "github.event.workflow_run.head_repository.full_name != github.repository",
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
        self.assertNotIn("uses: actions/checkout@", text)
        self.assertNotIn("path: target/", text)
        for contract in (
            'const workflowPath = ".github/workflows/lsp-bench.yml"',
            "run.repository?.full_name !== repository",
            'run.path === workflowPath || run.path?.startsWith(`${workflowPath}@`)',
            'run.event !== "pull_request"',
            "run_id: run.id",
            "artifact.workflow_run?.id === run.id",
            'artifact.name === expectedName',
            "artifact-ids: ${{ steps.artifact.outputs.artifact_id }}",
            "run-id: ${{ github.event.workflow_run.id }}",
            "${{ runner.temp }}/cross-lsp-comment",
            "find -P \"$REPORT_ROOT\" -type f -name report.md",
            'pull.head?.sha !== run.head_sha',
            'pull.head?.repo?.full_name !== headRepository',
            'pull.base?.repo?.full_name !== repository',
            'pull.head?.sha !== process.env.HEAD_SHA',
            'pull.base?.sha !== process.env.BASE_SHA',
            "header: cross-lsp-benchmark",
            "number_force: ${{ steps.pr.outputs.number }}",
        ):
            self.assertIn(contract, text)
        self.assertNotIn("lsp-bench-command", text)

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

        self.assertIn("needs: resolve", benchmark)
        self.assertIn("contents: read", benchmark)
        self.assertNotIn("issues: write", benchmark)
        self.assertNotIn("pull-requests: write", benchmark)
        self.assertIn(f"uses: {CHECKOUT_ACTION}", benchmark)
        self.assertIn(
            "ref: refs/pull/${{ needs.resolve.outputs.pr_number }}/head", benchmark
        )
        self.assertIn("persist-credentials: false", benchmark)
        self.assertIn('test "$(git rev-parse HEAD)" = "$HEAD_SHA"', benchmark)
        self.assertIn("run \\\n            --profile pr-smoke", benchmark)
        self.assertIn(SERVER_ARGS, benchmark)
        self.assertIn("--allow-failures", benchmark)
        self.assertIn("--solar-revision \"$HEAD_SHA\"", benchmark)
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
        self.assertNotIn("uses: actions/checkout@", comment)
        for contract in (
            "artifact-ids: ${{ needs.benchmark.outputs.artifact_id }}",
            "run-id: ${{ github.run_id }}",
            "${{ runner.temp }}/cross-lsp-manual-comment",
            "find -P \"$REPORT_ROOT\" -type f -name report.md",
            'pull.head?.sha !== process.env.HEAD_SHA',
            'pull.base?.sha !== process.env.BASE_SHA',
            "header: cross-lsp-command",
            "number_force: ${{ needs.resolve.outputs.pr_number }}",
        ):
            self.assertIn(contract, text)
        self.assertNotIn("cross-lsp-benchmark", text)
        self.assertNotIn("lsp-bench-command", text)


if __name__ == "__main__":
    unittest.main()
