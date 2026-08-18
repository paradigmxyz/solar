#!/usr/bin/env python3

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_PATH = ROOT / ".github/workflows/lsp-bench-command.yml"
WORKFLOW = WORKFLOW_PATH.read_text(encoding="utf-8")
BENCH_WORKFLOW = (ROOT / ".github/workflows/bench.yml").read_text(encoding="utf-8")


def job_block(name: str) -> str:
    jobs = WORKFLOW.split("\njobs:\n", 1)[1]
    match = re.search(
        rf"^  {re.escape(name)}:\n(?P<body>.*?)(?=^  [A-Za-z0-9_-]+:\n|\Z)",
        jobs,
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"job {name!r} is missing")
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


def job_permissions(name: str) -> dict[str, str]:
    job = job_block(name)
    match = re.search(
        r"^    permissions:\n(?P<body>(?:      [a-z-]+: (?:none|read|write)\n)+)",
        job,
        re.MULTILINE,
    )
    if match is None:
        raise AssertionError(f"job {name!r} has no explicit permissions")
    return dict(
        line.strip().split(": ", 1) for line in match.group("body").splitlines()
    )


def run_script(step: str) -> str:
    run = step.split("        run: |\n", 1)[1]
    if not all(not line or line.startswith("          ") for line in run.splitlines()):
        raise AssertionError("run block has unexpected indentation")
    return "\n".join(line[10:] for line in run.splitlines())


def github_script(step: str) -> str:
    script = step.split("          script: |\n", 1)[1]
    if not all(not line or line.startswith("            ") for line in script.splitlines()):
        raise AssertionError("github-script block has unexpected indentation")
    return "\n".join(line[12:] for line in script.splitlines())


def run_arbitration_script(current_number: int, other_number: int) -> dict[str, object]:
    script = github_script(
        step_block(job_block("arbitrate"), "Keep only the latest accepted request")
    )
    current_id = 100
    other_id = 101
    title = "LSP benchmark for paradigmxyz/solar#1195"
    runs = {
        current_id: {
            "id": current_id,
            "workflow_id": 7,
            "display_title": title,
            "event": "issue_comment",
            "run_number": current_number,
            "run_attempt": 1,
            "status": "in_progress",
        },
        other_id: {
            "id": other_id,
            "workflow_id": 7,
            "display_title": title,
            "event": "issue_comment",
            "run_number": other_number,
            "run_attempt": 1,
            "status": "in_progress",
        },
    }
    harness = f"""
const AsyncFunction = Object.getPrototypeOf(async function () {{}}).constructor;
const workflowScript = {json.dumps(script)};
const runs = new Map(
  Object.entries({json.dumps(runs)}).map(([id, run]) => [Number(id), run]),
);
const artifacts = [...runs.keys()].map((id) => ({{
  expired: false,
  workflow_run: {{ id }},
}}));
const cancelled = [];
const cancellationPolls = new Map();
const outputs = {{}};
const github = {{
  paginate: async () => artifacts,
  rest: {{
    actions: {{
      listArtifactsForRepo: async () => {{ throw new Error("paginate was bypassed"); }},
      getWorkflowRun: async ({{ run_id }}) => {{
        const run = runs.get(Number(run_id));
        if (!run) {{
          const error = new Error("run not found");
          error.status = 404;
          throw error;
        }}
        if (cancellationPolls.has(Number(run_id))) {{
          const polls = cancellationPolls.get(Number(run_id)) + 1;
          cancellationPolls.set(Number(run_id), polls);
          if (polls >= 2) {{
            run.status = "completed";
          }}
        }}
        return {{ data: run }};
      }},
      cancelWorkflowRun: async ({{ run_id }}) => {{
        cancelled.push(Number(run_id));
        cancellationPolls.set(Number(run_id), 0);
      }},
    }},
  }},
}};
const core = {{
  notice: () => {{}},
  setFailed: (message) => {{ throw new Error(`setFailed: ${{message}}`); }},
  setOutput: (name, value) => {{ outputs[name] = value; }},
}};
const context = {{ repo: {{ owner: "workflow", repo: "solar" }} }};

(async () => {{
  const execute = new AsyncFunction("github", "core", "context", workflowScript);
  await execute(github, core, context);
  process.stdout.write(JSON.stringify({{ cancelled, outputs }}));
}})().catch((error) => {{
  console.error(error.stack || String(error));
  process.exitCode = 1;
}});
"""
    environment = os.environ.copy()
    environment.update(
        {
            "CLAIM_KEY": "a" * 64,
            "PR_NUMBER": "1195",
            "RUN_ATTEMPT": "1",
            "RUN_ID": str(current_id),
            "RUN_NUMBER": str(current_number),
            "TARGET_REPOSITORY": "paradigmxyz/solar",
        }
    )
    completed = subprocess.run(
        ["node"],
        input=harness,
        text=True,
        capture_output=True,
        check=True,
        timeout=5,
        env=environment,
    )
    return json.loads(completed.stdout)


class TriggerAndResolutionTests(unittest.TestCase):
    def test_only_exact_pr_conversation_commands_are_accepted(self) -> None:
        header = WORKFLOW.split("\npermissions:", 1)[0]
        resolver = step_block(job_block("resolve"), "Validate request and freeze revisions")

        self.assertIn("  issue_comment:\n    types: [created]", header)
        self.assertNotIn("pull_request_target", WORKFLOW)
        self.assertNotIn("pull_request_review", header)
        self.assertNotIn("  pull_request:", header)
        self.assertEqual(WORKFLOW.count("github.event.comment.body == '/bench lsp'"), 1)
        self.assertNotIn("\nconcurrency:\n", WORKFLOW)
        self.assertIn("!context.payload.issue?.pull_request", resolver)
        self.assertIn('comment?.body !== "/bench lsp"', resolver)
        self.assertNotIn("comment?.body.trim", resolver)

        association_block = resolver.split("const allowedAssociations", 1)[1].split(
            "]);", 1
        )[0]
        self.assertEqual(
            set(re.findall(r'"([A-Z]+)"', association_block)),
            {"OWNER", "MEMBER", "COLLABORATOR", "CONTRIBUTOR"},
        )
        self.assertIn("allowedAssociations.has(comment.author_association)", resolver)
        claim_write = resolver.index("fs.writeFileSync")
        self.assertLess(resolver.index('comment?.body !== "/bench lsp"'), claim_write)
        self.assertLess(
            resolver.index("allowedAssociations.has(comment.author_association)"),
            claim_write,
        )

    def test_resolver_freezes_main_pr_head_and_trusted_default_branch(self) -> None:
        resolve_job = job_block("resolve")
        resolver = step_block(resolve_job, "Validate request and freeze revisions")

        self.assertIn("github.rest.pulls.get", resolver)
        self.assertIn('branch: "main"', resolver)
        self.assertIn("pull.state !== \"open\"", resolver)
        self.assertEqual(resolver.count("/^[0-9a-f]{40}$/"), 3)
        self.assertIn("owner: context.repo.owner", resolver)
        self.assertIn("repo: context.repo.repo", resolver)
        self.assertIn("branch: workflowRepository.default_branch", resolver)
        self.assertIn('core.setOutput("base_sha", main.commit.sha)', resolver)
        self.assertIn('core.setOutput("head_sha", pull.head.sha)', resolver)
        self.assertIn(
            'core.setOutput("trusted_sha", workflowDefault.commit.sha)', resolver
        )
        self.assertIn(
            "trusted_repo: ${{ steps.resolve.outputs.trusted_repo }}", resolve_job
        )
        self.assertIn(
            "trusted_sha: ${{ steps.resolve.outputs.trusted_sha }}", resolve_job
        )

    def test_manual_dispatch_cannot_execute_untrusted_code(self) -> None:
        header = WORKFLOW.split("\npermissions:", 1)[0]
        resolver = step_block(job_block("resolve"), "Validate request and freeze revisions")

        self.assertNotIn("\n  workflow_dispatch:", header)
        self.assertNotIn("inputs.", WORKFLOW)
        self.assertNotIn("DISPATCH_", WORKFLOW)
        self.assertNotIn('context.eventName === "workflow_dispatch"', resolver)
        self.assertNotIn("shouldComment", resolver)
        self.assertIn('core.setOutput("should_comment", "true")', resolver)
        self.assertIn('const allowedEvents = new Set(["issue_comment"]);', WORKFLOW)
        self.assertNotIn('"issue_comment", "workflow_dispatch"', WORKFLOW)

    def test_canonical_accepted_claims_make_later_requests_win(self) -> None:
        resolve = job_block("resolve")
        resolver = step_block(resolve, "Validate request and freeze revisions")
        upload = step_block(resolve, "Upload accepted request claim")
        arbitrate = job_block("arbitrate")
        script = step_block(arbitrate, "Keep only the latest accepted request")

        self.assertIn(
            '.update(`${target.full_name.toLowerCase()}\\0${pull.number}`)', resolver
        )
        self.assertIn('core.setOutput("claim_key", claimKey)', resolver)
        self.assertIn("name: lsp-bench-claim-${{ steps.resolve.outputs.claim_key }}", upload)
        self.assertIn("overwrite: true", upload)
        self.assertIn("needs: resolve", arbitrate)
        self.assertIn("CLAIM_KEY: ${{ needs.resolve.outputs.claim_key }}", script)
        self.assertIn("Number(process.env.RUN_NUMBER)", script)
        self.assertIn("Number(process.env.RUN_ATTEMPT)", script)
        self.assertIn("compareOrder(order, currentOrder) > 0", script)
        self.assertIn('core.setOutput("superseded", "true")', script)
        self.assertIn("compareOrder(order, currentOrder) < 0", script)
        self.assertIn("github.rest.actions.cancelWorkflowRun", script)
        self.assertIn("const waitForCompletion = async", script)
        self.assertIn("await waitForCompletion(run.id)", script)
        self.assertLess(
            script.index("compareOrder(order, currentOrder) > 0"),
            script.index("github.rest.actions.cancelWorkflowRun"),
        )

    @unittest.skipUnless(shutil.which("node"), "Node.js is required for workflow contract tests")
    def test_arbitration_is_independent_of_resolver_completion_order(self) -> None:
        older = run_arbitration_script(current_number=10, other_number=11)
        newer = run_arbitration_script(current_number=11, other_number=10)

        self.assertEqual(older, {"cancelled": [], "outputs": {"superseded": "true"}})
        self.assertEqual(
            newer,
            {"cancelled": [101], "outputs": {"superseded": "false"}},
        )


class PermissionAndCheckoutTests(unittest.TestCase):
    def test_jobs_have_the_minimum_declared_permissions(self) -> None:
        self.assertIn("\npermissions: {}\n", WORKFLOW)
        self.assertEqual(
            job_permissions("resolve"),
            {"contents": "read", "pull-requests": "read"},
        )
        self.assertEqual(job_permissions("arbitrate"), {"actions": "write"})
        self.assertEqual(job_permissions("queue-comment"), {"issues": "write"})
        self.assertEqual(job_permissions("compute"), {"contents": "read"})
        self.assertEqual(
            job_permissions("render"),
            {"actions": "read", "contents": "read", "issues": "write"},
        )
        self.assertEqual(WORKFLOW.count("issues: write"), 2)
        compute = job_block("compute")
        self.assertNotRegex(compute, r"\bsecrets\b")
        self.assertNotIn("github.token", compute)
        self.assertNotIn("GITHUB_TOKEN", compute)
        self.assertNotIn("GH_TOKEN", compute)

    def test_all_checkouts_are_credentialless_and_use_frozen_revisions(self) -> None:
        compute = job_block("compute")
        render = job_block("render")

        checkout_contracts = {
            "Checkout trusted benchmark adapter": (
                compute,
                "repository: ${{ needs.resolve.outputs.trusted_repo }}",
                "ref: ${{ needs.resolve.outputs.trusted_sha }}",
                "path: lsp-bench/trusted",
            ),
            "Checkout main revision": (
                compute,
                "repository: ${{ needs.resolve.outputs.base_repo }}",
                "ref: ${{ needs.resolve.outputs.base_sha }}",
                "path: lsp-bench/base",
            ),
            "Checkout pull request revision": (
                compute,
                "repository: ${{ needs.resolve.outputs.head_repo }}",
                "ref: ${{ needs.resolve.outputs.head_sha }}",
                "path: lsp-bench/head",
            ),
            "Checkout trusted renderer": (
                render,
                "repository: ${{ needs.resolve.outputs.trusted_repo }}",
                "ref: ${{ needs.resolve.outputs.trusted_sha }}",
                "path: lsp-bench/trusted",
            ),
        }
        self.assertEqual(WORKFLOW.count("uses: actions/checkout@"), len(checkout_contracts))
        for name, (job, repository, ref, path) in checkout_contracts.items():
            with self.subTest(name=name):
                checkout = step_block(job, name)
                self.assertEqual(checkout.count("uses: actions/checkout@"), 1)
                self.assertEqual(checkout.count("persist-credentials: false"), 1)
                self.assertIn(repository, checkout)
                self.assertIn(ref, checkout)
                self.assertIn(path, checkout)
        self.assertEqual(
            WORKFLOW.count("repository: ${{ needs.resolve.outputs.trusted_repo }}"), 2
        )
        self.assertEqual(
            WORKFLOW.count("ref: ${{ needs.resolve.outputs.trusted_sha }}"), 2
        )
        self.assertNotIn("github.workflow_sha", WORKFLOW)
        self.assertIn("ref: ${{ needs.resolve.outputs.base_sha }}", compute)
        self.assertIn("ref: ${{ needs.resolve.outputs.head_sha }}", compute)
        self.assertNotIn("lsp-bench/base", render)
        self.assertNotIn("lsp-bench/head", render)
        self.assertIn("clean: true", render)

    def test_renderer_is_fresh_trusted_code_and_never_runs_pr_code(self) -> None:
        queue = job_block("queue-comment")
        render = job_block("render")

        self.assertNotIn("actions/checkout@", queue)
        self.assertLess(
            render.index("name: Checkout trusted renderer"),
            render.index("name: Download raw benchmark artifact"),
        )
        self.assertNotIn("cargo build", render)
        self.assertNotIn("needs.resolve.outputs.head_repo }}\n          ref:", render)
        self.assertIn(
            'python3 "$GITHUB_WORKSPACE/lsp-bench/trusted/benches/lsp/benchmark.py" render',
            render,
        )

    def test_every_action_is_pinned_to_a_full_commit(self) -> None:
        actions = re.findall(r"^\s*uses:\s+([^\s#]+)", WORKFLOW, re.MULTILINE)

        self.assertTrue(actions)
        for action in actions:
            with self.subTest(action=action):
                self.assertRegex(action, r"@[0-9a-f]{40}\Z")


class ArtifactAndStatusTests(unittest.TestCase):
    def test_compute_uploads_only_the_manifest_covered_raw_tree(self) -> None:
        upload = step_block(job_block("compute"), "Upload raw benchmark artifact")
        expected_paths = {
            "${{ runner.temp }}/lsp-bench-artifact/raw/manifest.json",
            "${{ runner.temp }}/lsp-bench-artifact/raw/passes/base-first/config.json",
            "${{ runner.temp }}/lsp-bench-artifact/raw/passes/base-first/results.json",
            "${{ runner.temp }}/lsp-bench-artifact/raw/passes/head-first/config.json",
            "${{ runner.temp }}/lsp-bench-artifact/raw/passes/head-first/results.json",
        }
        paths = {
            line.strip()
            for line in upload.splitlines()
            if line.strip().startswith("${{ runner.temp }}/")
        }

        self.assertEqual(paths, expected_paths)
        self.assertNotIn("workflow.txt", WORKFLOW)
        self.assertIn("if: ${{ !cancelled() }}", upload)
        self.assertIn("if-no-files-found: error", upload)

    def test_untrusted_artifact_isolated_from_checkout_and_outputs(self) -> None:
        download = step_block(job_block("render"), "Download raw benchmark artifact")
        validate = step_block(job_block("render"), "Validate and render benchmark")

        self.assertEqual(
            WORKFLOW.count(
                "name: lsp-bench-raw-${{ github.run_id }}-${{ github.run_attempt }}"
            ),
            2,
        )
        self.assertIn("path: ${{ runner.temp }}/lsp-bench-download/raw", download)
        self.assertNotIn("github.workspace", download.lower())
        self.assertIn('--input "$RUNNER_TEMP/lsp-bench-download/raw"', validate)
        self.assertIn("continue-on-error: true", download)
        self.assertIn("continue-on-error: true", validate)
        self.assertIn(
            '--comparison "$RUNNER_TEMP/lsp-bench-render/comparison.json"', validate
        )
        self.assertIn('--report "$RUNNER_TEMP/lsp-bench-render/report.md"', validate)

    def test_incomplete_runs_publish_versioned_inconclusive_outputs_and_fail(self) -> None:
        render = job_block("render")
        stage = step_block(render, "Stage trusted report")
        failure = step_block(render, "Fail incomplete benchmark")

        for contract in (
            '"schema_version": 1',
            '"kind": "solar-lsp-benchmark-comparison"',
            '"overall": "inconclusive"',
            '"methods": []',
            '(output / "comparison.json").write_text',
            '(output / "report.md").write_text',
            '"$COMPUTE_RESULT" == success',
            '"$DOWNLOAD_OUTCOME" == success',
            '"$RENDER_OUTCOME" == success',
            "valid=true",
        ):
            self.assertIn(contract, stage)
        success_gate = stage.index('if [[ "$COMPUTE_RESULT" == success')
        copy_report = stage.index('cp "$RUNNER_TEMP/lsp-bench-render/report.md"')
        fallback = stage.index("python3 - <<'PY'")
        self.assertLess(success_gate, copy_report)
        self.assertLess(copy_report, fallback)
        self.assertIn("staged_dir=\"$RUNNER_TEMP/lsp-bench-report.staged\"", stage)
        self.assertIn('mv "$staged_dir" "$report_dir"', stage)
        self.assertIn(
            "if: ${{ !cancelled() && steps.stage.outputs.valid != 'true' }}",
            failure,
        )
        self.assertNotIn("regression", failure)

        upload = step_block(render, "Upload validated comparison")
        publish = step_block(render, "Publish sticky benchmark comment")
        self.assertIn("id: upload", upload)
        self.assertIn("steps.stage.outputs.valid == 'true'", publish)
        self.assertIn("steps.upload.outcome == 'success'", publish)

    def test_fallback_script_writes_both_versioned_outputs(self) -> None:
        stage = step_block(job_block("render"), "Stage trusted report")
        shell = run_script(stage)
        script = shell.split("python3 - <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        environment = os.environ.copy()
        environment.update(
            {
                "REPOSITORY": "paradigmxyz/solar",
                "HEAD_REPOSITORY": "contributor/solar",
                "WORKFLOW_REPOSITORY": "workflow/solar",
                "PR_NUMBER": "1195",
                "BASE_SHA": "1" * 40,
                "HEAD_SHA": "2" * 40,
                "RUN_URL": "https://github.com/workflow/solar/actions/runs/123",
                "FALLBACK_REASON": "benchmark did not complete successfully",
            }
        )

        with tempfile.TemporaryDirectory() as directory:
            environment["RUNNER_TEMP"] = directory
            output = Path(directory) / "lsp-bench-report"
            output.mkdir()
            subprocess.run(
                [sys.executable, "-"],
                input=script,
                text=True,
                check=True,
                timeout=5,
                env=environment,
            )
            comparison = json.loads((output / "comparison.json").read_text())

            self.assertEqual(comparison["schema_version"], 1)
            self.assertEqual(comparison["kind"], "solar-lsp-benchmark-comparison")
            self.assertEqual(comparison["overall"], "inconclusive")
            self.assertEqual(comparison["methods"], [])
            self.assertIn("**Overall:** `inconclusive`", (output / "report.md").read_text())

    def test_any_failed_stage_discards_partial_conclusive_outputs(self) -> None:
        stage = step_block(job_block("render"), "Stage trusted report")
        shell = run_script(stage)
        context = {
            "REPOSITORY": "paradigmxyz/solar",
            "HEAD_REPOSITORY": "contributor/solar",
            "WORKFLOW_REPOSITORY": "workflow/solar",
            "PR_NUMBER": "1195",
            "BASE_SHA": "1" * 40,
            "HEAD_SHA": "2" * 40,
            "RUN_URL": "https://github.com/workflow/solar/actions/runs/123",
        }

        for failed_stage in ("compute", "download", "render"):
            with self.subTest(failed_stage=failed_stage), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                rendered = root / "lsp-bench-render"
                rendered.mkdir()
                (rendered / "report.md").write_text("conclusive\n", encoding="utf-8")
                (rendered / "comparison.json").write_text(
                    json.dumps({"overall": "stable"}) + "\n", encoding="utf-8"
                )
                output_path = root / "step-output"
                environment = os.environ.copy()
                environment.update(context)
                environment.update(
                    {
                        "RUNNER_TEMP": directory,
                        "GITHUB_OUTPUT": str(output_path),
                        "COMPUTE_RESULT": (
                            "failure" if failed_stage == "compute" else "success"
                        ),
                        "DOWNLOAD_OUTCOME": (
                            "failure" if failed_stage == "download" else "success"
                        ),
                        "RENDER_OUTCOME": (
                            "failure" if failed_stage == "render" else "success"
                        ),
                    }
                )

                subprocess.run(
                    ["bash", "-c", shell],
                    check=True,
                    timeout=5,
                    env=environment,
                )

                comparison = json.loads(
                    (root / "lsp-bench-report/comparison.json").read_text()
                )
                self.assertEqual(comparison["overall"], "inconclusive")
                self.assertEqual(comparison["methods"], [])
                self.assertEqual(output_path.read_text(), "valid=false\n")
                self.assertNotEqual(
                    (root / "lsp-bench-report/report.md").read_text(), "conclusive\n"
                )

    def test_cancelled_runs_never_reach_renderer_or_comment_publication(self) -> None:
        render = job_block("render")
        publish = step_block(render, "Publish sticky benchmark comment")

        self.assertIn("!cancelled() &&\n      needs.resolve.result == 'success'", render)
        self.assertNotIn("if: always()", WORKFLOW)
        self.assertIn(
            "steps.stage.outputs.valid == 'true' &&\n          steps.upload.outcome == 'success'",
            publish,
        )
        for name in (
            "Stage trusted report",
            "Add comparison to job summary",
            "Upload validated comparison",
        ):
            self.assertIn("if: ${{ !cancelled() }}", step_block(render, name))


class ExecutionAndRemovalTests(unittest.TestCase):
    def test_build_and_upstream_runner_are_pinned(self) -> None:
        compute = job_block("compute")

        self.assertIn('toolchain: "1.96"', compute)
        self.assertEqual(
            compute.count(
                "cargo build --locked --release -p solar-compiler --bin solar"
            ),
            2,
        )
        self.assertIn('CARGO_TARGET_DIR="$RUNNER_TEMP/lsp-bench-target/base"', compute)
        self.assertIn('CARGO_TARGET_DIR="$RUNNER_TEMP/lsp-bench-target/head"', compute)
        self.assertIn("releases/download/v0.3.3/", compute)
        self.assertIn(
            "cf66d5237951046b0dd83726b86e0c8b23fc20fe3315f184fea48543337a23df",
            compute,
        )
        self.assertIn("runs-on: ubuntu-latest", compute)
        self.assertNotIn("depot-ubuntu-latest", compute)

    def test_existing_benchmark_workflow_only_adds_adapter_unit_tests(self) -> None:
        trigger = BENCH_WORKFLOW.split("\nenv:", 1)[0]

        self.assertNotIn("issue_comment", trigger)
        self.assertNotIn("pull_request_target", trigger)
        self.assertIn(
            '"$RUNNER_TEMP/schema-test/bin/python" -m unittest discover \\\n'
            "            -s benches/lsp -p 'test_*.py'",
            BENCH_WORKFLOW,
        )

    def test_legacy_lsp_stack_is_absent(self) -> None:
        workflow_names = {path.name for path in (ROOT / ".github/workflows").glob("lsp-bench*.yml")}

        self.assertEqual(workflow_names, {"lsp-bench-command.yml"})
        self.assertFalse((ROOT / "tools/lsp-bench").exists())
        self.assertFalse((ROOT / "LspBenchRunnerProbe.md").exists())
        for name in (
            "lsp-bench-authoritative-validate.jq",
            "lsp-bench-pr-comment.py",
            "lsp-bench-pr-config.py",
            "lsp-bench-probe.sh",
            "test_lsp_bench_pr_comment.py",
            "test_lsp_bench_pr_config.py",
            "test_lsp_bench_pr_workflows.py",
        ):
            with self.subTest(name=name):
                self.assertFalse((ROOT / ".github/scripts" / name).exists())


if __name__ == "__main__":
    unittest.main()
