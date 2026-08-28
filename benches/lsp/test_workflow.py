#!/usr/bin/env python3

from __future__ import annotations

import hashlib
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
UPLOAD_ARTIFACT_ACTION = (
    "uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a"
)
DOWNLOAD_ARTIFACT_ACTION = (
    "uses: actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c"
)


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
    if not all(
        not line or line.startswith("            ") for line in script.splitlines()
    ):
        raise AssertionError("github-script block has unexpected indentation")
    return "\n".join(line[12:] for line in script.splitlines())


def run_resolution_script(
    *,
    main_shas: list[str],
    pull_requests: list[dict[str, object]],
    merge_commits: dict[str, dict[str, object]],
    expect_success: bool = True,
) -> dict[str, object]:
    script = github_script(
        step_block(job_block("resolve"), "Validate request and freeze revisions")
    )
    harness = f"""
const AsyncFunction = Object.getPrototypeOf(async function () {{}}).constructor;
const workflowScript = {json.dumps(script)};
const mainShas = {json.dumps(main_shas)};
const pullRequests = {json.dumps(pull_requests)};
const mergeCommits = {json.dumps(merge_commits)};
let mainIndex = 0;
let pullIndex = 0;
const outputs = {{}};
const target = {{ full_name: "target/solar", owner: {{ login: "target" }}, name: "solar" }};
const workflow = {{ full_name: "workflow/solar", owner: {{ login: "workflow" }}, name: "solar", default_branch: "main" }};
const github = {{
  rest: {{
    repos: {{
      get: async ({{ owner }}) => ({{ data: owner === "target" ? target : workflow }}),
      getBranch: async ({{ owner }}) => ({{ data: {{ commit: {{ sha: owner === "target" ? mainShas[Math.min(mainIndex++, mainShas.length - 1)] : "4".repeat(40) }} }} }}),
      getCommit: async ({{ ref }}) => ({{ data: mergeCommits[ref] }}),
    }},
    pulls: {{
      get: async () => ({{ data: pullRequests[Math.min(pullIndex++, pullRequests.length - 1)] }}),
    }},
  }},
}};
const core = {{
  warning: () => {{}},
  setFailed: (message) => {{ throw new Error(`setFailed: ${{message}}`); }},
  setOutput: (name, value) => {{ outputs[name] = value; }},
}};
const context = {{
  payload: {{
    issue: {{ pull_request: {{}}, number: 12 }},
    comment: {{ body: "/bench lsp", author_association: "OWNER" }},
    repository: {{ full_name: "target/solar" }},
  }},
  repo: {{ owner: "workflow", repo: "solar" }},
}};
globalThis.setTimeout = (callback) => {{ callback(); return 0; }};
(async () => {{
  const execute = new AsyncFunction("github", "core", "context", workflowScript);
  await execute(github, core, context);
  process.stdout.write(JSON.stringify({{ outputs }}));
}})().catch((error) => {{
  console.error(error.stack || String(error));
  process.exitCode = 1;
}});
"""
    with tempfile.TemporaryDirectory(prefix="lsp-resolve-") as directory:
        environment = os.environ.copy()
        environment.update(
            {
                "RUNNER_TEMP": directory,
                "RUN_URL": "https://github.com/workflow/solar/actions/runs/100",
                "RUN_ID": "100",
                "RUN_NUMBER": "1",
                "RUN_ATTEMPT": "1",
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
        if not expect_success:
            if completed.returncode == 0:
                raise AssertionError("resolver unexpectedly accepted the request")
            return {"error": completed.stderr}
        completed.check_returncode()
        result = json.loads(completed.stdout)
        result["claim"] = json.loads(
            (Path(directory) / "lsp-bench-claim/claim.json").read_text()
        )
        return result


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
        resolver = step_block(
            job_block("resolve"), "Validate request and freeze revisions"
        )

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

    def test_resolver_freezes_main_pr_head_and_test_merge(self) -> None:
        resolve_job = job_block("resolve")
        resolver = step_block(resolve_job, "Validate request and freeze revisions")

        self.assertIn("github.rest.pulls.get", resolver)
        self.assertIn('branch: "main"', resolver)
        self.assertIn('pull.state !== "open"', resolver)
        self.assertIn('pull.base?.ref !== "main"', resolver)
        self.assertIn("const MERGEABLE_POLLS", resolver)
        self.assertIn("pull.mergeable === null", resolver)
        self.assertIn("pull.mergeable !== true", resolver)
        self.assertIn("github.rest.repos.getCommit", resolver)
        self.assertIn("parents.length !== 2", resolver)
        self.assertIn("parents[0] !== mainSha", resolver)
        self.assertIn("parents[1] !== prHeadSha", resolver)
        self.assertIn("MAX_RESOLUTION_ATTEMPTS", resolver)
        self.assertIn("resolution-race", resolver)
        self.assertNotIn("compareCommitsWithBasehead", resolver)
        self.assertIn('core.setOutput("main_sha", frozen.mainSha)', resolver)
        self.assertIn('core.setOutput("pr_head_sha", frozen.prHeadSha)', resolver)
        self.assertIn(
            'core.setOutput("merge_candidate_sha", frozen.mergeCandidateSha)', resolver
        )
        self.assertNotIn("merge_base_sha", resolver)
        self.assertNotIn("merge-base-head", resolver)
        self.assertIn("owner: context.repo.owner", resolver)
        self.assertIn("repo: context.repo.repo", resolver)
        self.assertIn("branch: workflowRepository.default_branch", resolver)
        self.assertIn(
            'core.setOutput("trusted_sha", workflowDefault.commit.sha)', resolver
        )
        self.assertIn(
            "trusted_repo: ${{ steps.resolve.outputs.trusted_repo }}", resolve_job
        )
        self.assertIn(
            "trusted_sha: ${{ steps.resolve.outputs.trusted_sha }}", resolve_job
        )

    def test_queue_comment_identifies_frozen_d_f_m_attribution(self) -> None:
        queue = job_block("queue-comment")

        self.assertIn("MAIN_SHA: ${{ needs.resolve.outputs.main_sha }}", queue)
        self.assertIn("PR_HEAD_SHA: ${{ needs.resolve.outputs.pr_head_sha }}", queue)
        self.assertIn(
            "MERGE_CANDIDATE_SHA: ${{ needs.resolve.outputs.merge_candidate_sha }}",
            queue,
        )
        self.assertIn("Queued frozen comparison of main", queue)
        self.assertIn("(D)", queue)
        self.assertIn("(F)", queue)
        self.assertIn("(M)", queue)
        self.assertNotIn("merge-base", queue)

    def test_manual_dispatch_cannot_execute_untrusted_code(self) -> None:
        header = WORKFLOW.split("\npermissions:", 1)[0]
        resolver = step_block(
            job_block("resolve"), "Validate request and freeze revisions"
        )

        self.assertNotIn("\n  workflow_dispatch:", header)
        self.assertNotIn("inputs.", WORKFLOW)
        self.assertNotIn("DISPATCH_", WORKFLOW)
        self.assertNotIn('context.eventName === "workflow_dispatch"', resolver)
        self.assertNotIn("shouldComment", resolver)
        self.assertNotIn("should_comment", WORKFLOW)
        self.assertIn('const allowedEvents = new Set(["issue_comment"]);', WORKFLOW)
        self.assertNotIn('"issue_comment", "workflow_dispatch"', WORKFLOW)

    def test_canonical_accepted_claims_make_later_requests_win(self) -> None:
        resolve = job_block("resolve")
        resolver = step_block(resolve, "Validate request and freeze revisions")
        upload = step_block(resolve, "Upload accepted request claim")
        arbitrate = job_block("arbitrate")
        script = step_block(arbitrate, "Keep only the latest accepted request")

        self.assertIn(
            ".update(`${target.full_name.toLowerCase()}\\0${requestedNumber}`)",
            resolver,
        )
        self.assertIn('core.setOutput("claim_key", claimKey)', resolver)
        self.assertIn(
            "name: lsp-bench-claim-${{ steps.resolve.outputs.claim_key }}", upload
        )
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
        self.assertNotIn("downloadArtifact", script)
        self.assertLess(
            script.index("compareOrder(order, currentOrder) > 0"),
            script.index("github.rest.actions.cancelWorkflowRun"),
        )

    @unittest.skipUnless(
        shutil.which("node"), "Node.js is required for workflow contract tests"
    )
    def test_resolver_freezes_exact_d_f_m_and_writes_minimal_claim(self) -> None:
        main_sha = "1" * 40
        pr_head_sha = "2" * 40
        merge_candidate_sha = "3" * 40
        pull = {
            "state": "open",
            "base": {"repo": {"full_name": "target/solar"}, "ref": "main"},
            "head": {
                "repo": {"full_name": "contributor/solar"},
                "sha": pr_head_sha,
            },
            "mergeable": True,
            "merge_commit_sha": merge_candidate_sha,
        }
        waiting = {**pull, "mergeable": None, "merge_commit_sha": None}
        result = run_resolution_script(
            main_shas=[main_sha, main_sha],
            pull_requests=[waiting, pull, pull],
            merge_commits={
                merge_candidate_sha: {
                    "sha": merge_candidate_sha,
                    "parents": [{"sha": main_sha}, {"sha": pr_head_sha}],
                }
            },
        )

        self.assertEqual(result["outputs"]["main_sha"], main_sha)
        self.assertEqual(result["outputs"]["pr_head_sha"], pr_head_sha)
        self.assertEqual(result["outputs"]["merge_candidate_sha"], merge_candidate_sha)
        self.assertEqual(
            result["claim"],
            {
                "schema_version": 1,
                "kind": "solar-lsp-benchmark-request-claim",
                "claim_key": result["outputs"]["claim_key"],
            },
        )

    @unittest.skipUnless(
        shutil.which("node"), "Node.js is required for workflow contract tests"
    )
    def test_resolver_retries_the_entire_group_when_main_moves(self) -> None:
        first_main = "1" * 40
        second_main = "2" * 40
        pr_head_sha = "3" * 40
        first_merge = "4" * 40
        second_merge = "5" * 40

        def pull(merge_candidate_sha: str) -> dict[str, object]:
            return {
                "state": "open",
                "base": {"repo": {"full_name": "target/solar"}, "ref": "main"},
                "head": {
                    "repo": {"full_name": "contributor/solar"},
                    "sha": pr_head_sha,
                },
                "mergeable": True,
                "merge_commit_sha": merge_candidate_sha,
            }

        result = run_resolution_script(
            main_shas=[first_main, second_main, second_main, second_main],
            pull_requests=[
                pull(first_merge),
                pull(first_merge),
                pull(second_merge),
                pull(second_merge),
            ],
            merge_commits={
                first_merge: {
                    "sha": first_merge,
                    "parents": [{"sha": first_main}, {"sha": pr_head_sha}],
                },
                second_merge: {
                    "sha": second_merge,
                    "parents": [{"sha": second_main}, {"sha": pr_head_sha}],
                },
            },
        )

        self.assertEqual(result["outputs"]["main_sha"], second_main)
        self.assertEqual(result["outputs"]["merge_candidate_sha"], second_merge)

    @unittest.skipUnless(
        shutil.which("node"), "Node.js is required for workflow contract tests"
    )
    def test_resolver_rejects_invalid_test_merge_contracts(self) -> None:
        main_sha = "1" * 40
        pr_head_sha = "2" * 40
        merge_candidate_sha = "3" * 40
        valid_pull = {
            "state": "open",
            "base": {"repo": {"full_name": "target/solar"}, "ref": "main"},
            "head": {
                "repo": {"full_name": "contributor/solar"},
                "sha": pr_head_sha,
            },
            "mergeable": True,
            "merge_commit_sha": merge_candidate_sha,
        }
        cases = {
            "wrong parents": (
                valid_pull,
                {
                    merge_candidate_sha: {
                        "sha": merge_candidate_sha,
                        "parents": [{"sha": pr_head_sha}, {"sha": main_sha}],
                    }
                },
                "parents are not exactly",
            ),
            "missing merge commit": (
                {**valid_pull, "merge_commit_sha": None},
                {},
                "no complete test merge commit SHA",
            ),
            "non-main target": (
                {
                    **valid_pull,
                    "base": {
                        "repo": {"full_name": "target/solar"},
                        "ref": "develop",
                    },
                },
                {},
                "must target the requested repository's `main`",
            ),
        }

        for name, (pull, commits, message) in cases.items():
            with self.subTest(name=name):
                result = run_resolution_script(
                    main_shas=[main_sha],
                    pull_requests=[pull],
                    merge_commits=commits,
                    expect_success=False,
                )
                self.assertIn(message, result["error"])

    @unittest.skipUnless(
        shutil.which("node"), "Node.js is required for workflow contract tests"
    )
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
        self.assertEqual(
            job_permissions("queue-comment"),
            {"issues": "write", "pull-requests": "write"},
        )
        self.assertEqual(job_permissions("build_base"), {"contents": "read"})
        self.assertEqual(job_permissions("build_candidate"), {"contents": "read"})
        self.assertEqual(job_permissions("compute"), {"contents": "read"})
        self.assertEqual(
            job_permissions("render"),
            {
                "actions": "read",
                "contents": "read",
                "issues": "write",
                "pull-requests": "write",
            },
        )
        self.assertEqual(WORKFLOW.count("issues: write"), 2)
        for name in ("build_base", "build_candidate", "compute"):
            with self.subTest(name=name):
                job = job_block(name)
                self.assertNotRegex(job, r"\bsecrets\b")
                self.assertNotIn("github.token", job)
                self.assertNotIn("GITHUB_TOKEN", job)
                self.assertNotIn("GH_TOKEN", job)
        workflow_lower = WORKFLOW.lower()
        self.assertNotIn("actions/cache@", workflow_lower)
        self.assertNotIn("rust-cache@", workflow_lower)
        self.assertNotIn("sccache", workflow_lower)
        self.assertIsNone(re.search(r"^\s+cache:", WORKFLOW, re.MULTILINE))

    def test_all_checkouts_are_credentialless_and_use_frozen_revisions(self) -> None:
        build_base = job_block("build_base")
        build_candidate = job_block("build_candidate")
        compute = job_block("compute")
        render = job_block("render")

        checkout_contracts = {
            "Checkout trusted benchmark adapter": (
                compute,
                "repository: ${{ needs.resolve.outputs.trusted_repo }}",
                "ref: ${{ needs.resolve.outputs.trusted_sha }}",
                "path: lsp-bench/trusted",
            ),
            "Checkout main revision (D)": (
                build_base,
                "repository: ${{ needs.resolve.outputs.repository }}",
                "ref: ${{ needs.resolve.outputs.main_sha }}",
                "path: lsp-bench/base",
            ),
            "Checkout test merge revision (M)": (
                build_candidate,
                "repository: ${{ needs.resolve.outputs.repository }}",
                "ref: ${{ needs.resolve.outputs.merge_candidate_sha }}",
                "path: lsp-bench/candidate",
            ),
            "Checkout trusted renderer": (
                render,
                "repository: ${{ needs.resolve.outputs.trusted_repo }}",
                "ref: ${{ needs.resolve.outputs.trusted_sha }}",
                "path: lsp-bench/trusted",
            ),
        }
        self.assertEqual(
            WORKFLOW.count("uses: actions/checkout@"), len(checkout_contracts)
        )
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
        self.assertIn("ref: ${{ needs.resolve.outputs.main_sha }}", build_base)
        self.assertIn(
            "ref: ${{ needs.resolve.outputs.merge_candidate_sha }}", build_candidate
        )
        self.assertNotIn("Checkout main revision", compute)
        self.assertNotIn("Checkout test merge revision", compute)
        self.assertIn('--main-sha "$MAIN_SHA"', compute)
        self.assertIn('--merge-candidate-sha "$MERGE_CANDIDATE_SHA"', compute)
        validate = step_block(render, "Validate and render benchmark")
        self.assertIn('--main-sha "$MAIN_SHA"', validate)
        self.assertIn('--merge-candidate-sha "$MERGE_CANDIDATE_SHA"', validate)
        self.assertNotIn("--current-main-sha", validate)
        self.assertNotIn("--current-pr-head-sha", validate)
        self.assertNotIn("lsp-bench/base", render)
        self.assertNotIn("lsp-bench/head", render)
        self.assertIn("clean: true", render)

    def test_renderer_is_fresh_trusted_code_and_never_runs_pr_code(self) -> None:
        queue = job_block("queue-comment")
        render = job_block("render")
        current = step_block(render, "Query current PR revisions")
        validate = step_block(render, "Validate and render benchmark")

        self.assertNotIn("actions/checkout@", queue)
        self.assertLess(
            render.index("name: Checkout trusted renderer"),
            render.index("name: Download raw benchmark artifact"),
        )
        self.assertLess(
            render.index("name: Download raw benchmark artifact"),
            render.index("name: Query current PR revisions"),
        )
        self.assertLess(
            render.index("name: Query current PR revisions"),
            render.index("name: Validate and render benchmark"),
        )
        self.assertNotIn("cargo build", render)
        self.assertNotIn(
            "needs.resolve.outputs.pr_head_repository }}\n          ref:", render
        )
        self.assertIn("github.rest.repos.getBranch", current)
        self.assertIn("github.rest.pulls.get", current)
        self.assertIn("continue-on-error: true", current)
        self.assertIn(
            "CURRENT_MAIN_SHA: ${{ steps.current_state.outputs.main_sha }}", validate
        )
        self.assertIn(
            "CURRENT_PR_HEAD_SHA: ${{ steps.current_state.outputs.pr_head_sha }}",
            validate,
        )
        self.assertNotIn("steps.current_state.outputs", run_script(validate))
        self.assertIn(
            'python3 "$GITHUB_WORKSPACE/lsp-bench/trusted/benches/lsp/benchmark.py" render',
            render,
        )
        self.assertNotIn("lsp-bench-base-binary", render)
        self.assertNotIn("lsp-bench-candidate-binary", render)
        self.assertNotIn("--base-binary", render)
        self.assertNotIn("--head-binary", render)

    def test_every_action_is_pinned_to_a_full_commit(self) -> None:
        actions = re.findall(r"^\s*uses:\s+([^\s#]+)", WORKFLOW, re.MULTILINE)

        self.assertTrue(actions)
        for action in actions:
            with self.subTest(action=action):
                self.assertRegex(action, r"@[0-9a-f]{40}\Z")


class ArtifactAndStatusTests(unittest.TestCase):
    def test_build_jobs_upload_role_specific_binaries(self) -> None:
        contracts = {
            "build_base": (
                "Build main compiler (D)",
                "Upload main compiler",
                "lsp-bench-base-binary-${{ github.run_id }}-${{ github.run_attempt }}",
                "${{ runner.temp }}/lsp-bench-bin/base-solar",
                '"$RUNNER_TEMP/lsp-bench-bin/base-solar"',
            ),
            "build_candidate": (
                "Build test merge compiler (M)",
                "Upload test merge compiler",
                "lsp-bench-candidate-binary-${{ github.run_id }}-${{ github.run_attempt }}",
                "${{ runner.temp }}/lsp-bench-bin/candidate-solar",
                '"$RUNNER_TEMP/lsp-bench-bin/candidate-solar"',
            ),
        }

        for job_name, contract in contracts.items():
            with self.subTest(job=job_name):
                build_name, upload_name, artifact_name, path, shell_path = contract
                job = job_block(job_name)
                build = step_block(job, build_name)
                upload = step_block(job, upload_name)
                self.assertIn(
                    "artifact_id: ${{ steps.upload.outputs.artifact-id }}", job
                )
                self.assertEqual(job.count("uses: actions/upload-artifact@"), 1)
                self.assertEqual(upload.count(UPLOAD_ARTIFACT_ACTION), 1)
                self.assertIn(shell_path, build)
                self.assertIn("id: upload", upload)
                self.assertIn(f"name: {artifact_name}", upload)
                self.assertIn(f"path: {path}", upload)
                self.assertIn("retention-days: 1", upload)
                self.assertIn("if-no-files-found: error", upload)
                self.assertNotIn("overwrite:", upload)
                self.assertNotIn("lsp-bench-target", upload)
                self.assertEqual(WORKFLOW.count(f"name: {artifact_name}"), 1)
                self.assertLess(
                    job.index(f"name: {build_name}"),
                    job.index(f"name: {upload_name}"),
                )

    def test_compute_downloads_binaries_outside_the_trusted_checkout(self) -> None:
        compute = job_block("compute")
        contracts = {
            "Download main compiler": (
                "${{ needs.build_base.outputs.artifact_id }}",
                "${{ runner.temp }}/lsp-bench-bin",
            ),
            "Download test merge compiler": (
                "${{ needs.build_candidate.outputs.artifact_id }}",
                "${{ runner.temp }}/lsp-bench-bin",
            ),
        }

        for step_name, (artifact_id, path) in contracts.items():
            with self.subTest(step=step_name):
                download = step_block(compute, step_name)
                self.assertEqual(download.count(DOWNLOAD_ARTIFACT_ACTION), 1)
                self.assertIn(f"artifact-ids: {artifact_id}", download)
                self.assertIn(f"path: {path}", download)
                self.assertNotIn("github.workspace", download.lower())
                self.assertNotIn("github.run_attempt", download)
                self.assertNotIn("github-token:", download)
                self.assertNotIn("repository:", download)
                self.assertNotIn("run-id:", download)
                self.assertNotIn("pattern:", download)
                self.assertLess(
                    compute.index(f"name: {step_name}"),
                    compute.index("name: Prepare compiler binaries"),
                )
        prepare = step_block(compute, "Prepare compiler binaries")
        self.assertIn('chmod 0755 "$RUNNER_TEMP/lsp-bench-bin/base-solar"', prepare)
        self.assertIn(
            'chmod 0755 "$RUNNER_TEMP/lsp-bench-bin/candidate-solar"', prepare
        )
        self.assertIn('test -x "$RUNNER_TEMP/lsp-bench-bin/base-solar"', prepare)
        self.assertIn('test -x "$RUNNER_TEMP/lsp-bench-bin/candidate-solar"', prepare)
        self.assertLess(
            compute.index("name: Download main compiler"),
            compute.index("name: Run LSP comparison"),
        )
        self.assertLess(
            compute.index("name: Download test merge compiler"),
            compute.index("name: Run LSP comparison"),
        )
        self.assertLess(
            compute.index("name: Prepare compiler binaries"),
            compute.index("name: Run LSP comparison"),
        )
        run = step_block(compute, "Run LSP comparison")
        self.assertIn('--base-binary "$RUNNER_TEMP/lsp-bench-bin/base-solar"', run)
        self.assertIn('--head-binary "$RUNNER_TEMP/lsp-bench-bin/candidate-solar"', run)

    def test_compute_uploads_only_the_manifest_covered_raw_tree(self) -> None:
        upload = step_block(job_block("compute"), "Upload raw benchmark artifact")
        expected_paths = {"${{ runner.temp }}/lsp-bench-artifact/raw/manifest.json"}
        expected_paths.update(
            f"${{{{ runner.temp }}}}/lsp-bench-artifact/raw/passes/{order}/{session}/{name}.json"
            for order in ("base-first", "head-first")
            for session in range(1, 6)
            for name in ("config", "results")
        )
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

    def test_incomplete_runs_publish_versioned_inconclusive_outputs_and_fail(
        self,
    ) -> None:
        render = job_block("render")
        stage = step_block(render, "Stage trusted report")
        failure = step_block(render, "Fail incomplete benchmark")

        for contract in (
            "COMPUTE_RESULT: ${{ needs.compute.result }}",
            '"schema_version": 3',
            '"kind": "solar-lsp-benchmark-comparison"',
            '"overall": "inconclusive"',
            '"methods": []',
            '"threshold_absolute_ms": 1.0',
            '"confidence_level": 0.95',
            '(output / "comparison.json").write_text',
            '(output / "report.md").write_text',
            '"$COMPUTE_RESULT" == success',
            '"$DOWNLOAD_OUTCOME" == success',
            '"$current_state_outcome" == success',
            '"$RENDER_OUTCOME" == success',
            "conclusive=true",
            "publishable=true",
        ):
            self.assertIn(contract, stage)
        success_gate = stage.index('if [[ "$COMPUTE_RESULT" == success')
        copy_report = stage.index('cp "$RUNNER_TEMP/lsp-bench-render/report.md"')
        fallback = stage.index("python3 - <<'PY'")
        self.assertLess(success_gate, copy_report)
        self.assertLess(copy_report, fallback)
        self.assertIn('staged_dir="$RUNNER_TEMP/lsp-bench-report.staged"', stage)
        self.assertIn('mv "$staged_dir" "$report_dir"', stage)
        self.assertIn(
            "if: ${{ !cancelled() && steps.stage.outputs.conclusive != 'true' }}",
            failure,
        )
        self.assertNotIn("regression", failure)
        self.assertNotIn("freshness", failure)

        upload = step_block(render, "Upload validated comparison")
        publish = step_block(render, "Publish sticky benchmark comment")
        self.assertIn("id: upload", upload)
        self.assertIn("steps.stage.outputs.publishable == 'true'", upload)
        self.assertIn("steps.stage.outputs.publishable == 'true'", publish)
        self.assertNotIn("steps.stage.outputs.conclusive", publish)
        self.assertIn("steps.upload.outcome == 'success'", publish)
        self.assertLess(
            render.index("name: Upload validated comparison"),
            render.index("name: Publish sticky benchmark comment"),
        )
        self.assertLess(
            render.index("name: Publish sticky benchmark comment"),
            render.index("name: Fail incomplete benchmark"),
        )

    def test_fallback_script_writes_both_versioned_outputs(self) -> None:
        stage = step_block(job_block("render"), "Stage trusted report")
        shell = run_script(stage)
        script = shell.split("python3 - <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        environment = os.environ.copy()
        environment.update(
            {
                "TARGET_REPOSITORY": "paradigmxyz/solar",
                "PR_HEAD_REPOSITORY": "contributor/solar",
                "WORKFLOW_REPOSITORY": "workflow/solar",
                "PR_NUMBER": "1195",
                "MAIN_SHA": "1" * 40,
                "PR_HEAD_SHA": "2" * 40,
                "MERGE_CANDIDATE_SHA": "3" * 40,
                "RUN_URL": "https://github.com/workflow/solar/actions/runs/123",
                "FALLBACK_REASON": "benchmark did not complete successfully",
            }
        )

        with tempfile.TemporaryDirectory() as directory:
            environment["RUNNER_TEMP"] = directory
            output = Path(directory) / "lsp-bench-report"
            output.mkdir()
            environment["REPORT_DIR"] = str(output)
            subprocess.run(
                [sys.executable, "-"],
                input=script,
                text=True,
                check=True,
                timeout=5,
                env=environment,
            )
            comparison = json.loads((output / "comparison.json").read_text())

            self.assertEqual(comparison["schema_version"], 3)
            self.assertEqual(comparison["kind"], "solar-lsp-benchmark-comparison")
            self.assertEqual(comparison["comparison_mode"], "main-merge-candidate")
            self.assertEqual(comparison["main_sha"], "1" * 40)
            self.assertEqual(comparison["pr_head_sha"], "2" * 40)
            self.assertEqual(comparison["merge_candidate_sha"], "3" * 40)
            self.assertNotIn("merge_base_sha", comparison)
            self.assertEqual(comparison["overall"], "inconclusive")
            self.assertEqual(comparison["methods"], [])
            self.assertEqual(comparison["threshold_percent"], 10.0)
            self.assertEqual(comparison["threshold_absolute_ms"], 1.0)
            self.assertEqual(comparison["confidence_level"], 0.95)
            self.assertIn(
                "**Overall:** `inconclusive`", (output / "report.md").read_text()
            )

    def test_any_failed_stage_discards_partial_conclusive_outputs(self) -> None:
        stage = step_block(job_block("render"), "Stage trusted report")
        shell = run_script(stage)
        context = {
            "TARGET_REPOSITORY": "paradigmxyz/solar",
            "PR_HEAD_REPOSITORY": "contributor/solar",
            "WORKFLOW_REPOSITORY": "workflow/solar",
            "PR_NUMBER": "1195",
            "MAIN_SHA": "1" * 40,
            "PR_HEAD_SHA": "2" * 40,
            "MERGE_CANDIDATE_SHA": "3" * 40,
            "RUN_URL": "https://github.com/workflow/solar/actions/runs/123",
            "CURRENT_STATE_OUTCOME": "success",
            "CURRENT_MAIN_SHA": "1" * 40,
            "CURRENT_PR_HEAD_SHA": "2" * 40,
        }

        for failed_stage in (
            "compute",
            "compute_skipped",
            "download",
            "current_state",
            "render",
        ):
            with (
                self.subTest(failed_stage=failed_stage),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                rendered = root / "lsp-bench-render"
                rendered.mkdir()
                (rendered / "report.md").write_text("conclusive\n", encoding="utf-8")
                (rendered / "comparison.json").write_text(
                    json.dumps({"overall": "stable"}) + "\n", encoding="utf-8"
                )
                output_path = root / "step-output"
                compute_result = {
                    "compute": "failure",
                    "compute_skipped": "skipped",
                }.get(failed_stage, "success")
                environment = os.environ.copy()
                environment.update(context)
                environment.update(
                    {
                        "RUNNER_TEMP": directory,
                        "GITHUB_OUTPUT": str(output_path),
                        "COMPUTE_RESULT": compute_result,
                        "DOWNLOAD_OUTCOME": (
                            "failure" if failed_stage == "download" else "success"
                        ),
                        "CURRENT_STATE_OUTCOME": (
                            "failure" if failed_stage == "current_state" else "success"
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
                self.assertEqual(
                    output_path.read_text(),
                    "conclusive=false\npublishable=true\n",
                )
                self.assertNotEqual(
                    (root / "lsp-bench-report/report.md").read_text(), "conclusive\n"
                )

    def test_complete_stale_report_is_still_conclusive_and_publishable(self) -> None:
        stage = step_block(job_block("render"), "Stage trusted report")
        shell = run_script(stage)
        context = {
            "TARGET_REPOSITORY": "paradigmxyz/solar",
            "PR_HEAD_REPOSITORY": "contributor/solar",
            "WORKFLOW_REPOSITORY": "workflow/solar",
            "PR_NUMBER": "1195",
            "MAIN_SHA": "1" * 40,
            "PR_HEAD_SHA": "2" * 40,
            "MERGE_CANDIDATE_SHA": "3" * 40,
            "RUN_URL": "https://github.com/workflow/solar/actions/runs/123",
            "CURRENT_STATE_OUTCOME": "success",
            "CURRENT_MAIN_SHA": "1" * 40,
            "CURRENT_PR_HEAD_SHA": "2" * 40,
            "COMPUTE_RESULT": "success",
            "DOWNLOAD_OUTCOME": "success",
            "RENDER_OUTCOME": "success",
        }

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            rendered = root / "lsp-bench-render"
            rendered.mkdir()
            (rendered / "report.md").write_text("stale but valid\n", encoding="utf-8")
            (rendered / "comparison.json").write_text(
                json.dumps({"overall": "stable", "freshness": "main-advanced"}) + "\n",
                encoding="utf-8",
            )
            output_path = root / "step-output"
            environment = os.environ.copy()
            environment.update(context)
            environment.update(
                {
                    "RUNNER_TEMP": directory,
                    "GITHUB_OUTPUT": str(output_path),
                }
            )

            subprocess.run(
                ["bash", "-c", shell],
                check=True,
                timeout=5,
                env=environment,
            )

            self.assertEqual(
                output_path.read_text(),
                "conclusive=true\npublishable=true\n",
            )
            self.assertEqual(
                (root / "lsp-bench-report/report.md").read_text(),
                "stale but valid\n",
            )

    def test_cancelled_runs_never_reach_renderer_or_comment_publication(self) -> None:
        render = job_block("render")
        publish = step_block(render, "Publish sticky benchmark comment")

        self.assertIn("needs: [resolve, arbitrate, queue-comment, compute]", render)
        self.assertIn(
            "!cancelled() &&\n      needs.resolve.result == 'success'", render
        )
        self.assertNotIn("needs.compute.result == 'success'", render)
        self.assertNotIn("if: always()", WORKFLOW)
        self.assertIn(
            "steps.stage.outputs.publishable == 'true' &&\n          steps.upload.outcome == 'success'",
            publish,
        )
        self.assertIn(
            "if: ${{ !cancelled() }}", step_block(render, "Stage trusted report")
        )
        for name in ("Add comparison to job summary", "Upload validated comparison"):
            step = step_block(render, name)
            self.assertIn("!cancelled()", step)
            self.assertIn("steps.stage.outputs.publishable == 'true'", step)


class ExecutionAndRemovalTests(unittest.TestCase):
    def test_redundant_workflow_state_is_removed(self) -> None:
        compute = job_block("compute")
        run = step_block(compute, "Run LSP comparison")
        render = job_block("render")
        validate = step_block(render, "Validate and render benchmark")
        stage = step_block(render, "Stage trusted report")

        self.assertNotIn("Initialize raw artifact", compute)
        self.assertNotIn("should_comment", WORKFLOW)
        self.assertNotIn("\n          REPOSITORY:", WORKFLOW)
        for step in (run, validate, stage):
            self.assertIn(
                "TARGET_REPOSITORY: ${{ needs.resolve.outputs.repository }}", step
            )
        for step in (run, validate):
            self.assertIn('--repository "$TARGET_REPOSITORY"', step)
            self.assertIn('--pr-head-repository "$PR_HEAD_REPOSITORY"', step)
            self.assertIn('--workflow-repository "$WORKFLOW_REPOSITORY"', step)

    def test_parallel_builds_and_direct_runner_are_pinned(self) -> None:
        build_base = job_block("build_base")
        build_candidate = job_block("build_candidate")
        compute = job_block("compute")
        upstream = json.loads((ROOT / "benches/lsp/upstream.json").read_text())
        adapter_path = ROOT / upstream["adapter"]["path"]
        source_url = upstream["source"]["url"].replace(upstream["commit"], "$commit")
        version = (
            f"lsp-bench {upstream['version']}+commit."
            f"{upstream['commit'][:7]}.linux.x86_64"
        )

        self.assertIn("needs: [resolve, arbitrate]", build_base)
        self.assertIn("needs: [resolve, arbitrate]", build_candidate)
        self.assertNotIn("build_candidate", build_base)
        self.assertNotIn("build_base", build_candidate)
        for job in (build_base, build_candidate):
            for gate in (
                "!cancelled()",
                "needs.resolve.result == 'success'",
                "needs.arbitrate.result == 'success'",
                "needs.arbitrate.outputs.superseded == 'false'",
            ):
                self.assertIn(gate, job)
        self.assertIn(
            "needs: [resolve, arbitrate, build_base, build_candidate]", compute
        )
        self.assertIn("needs.build_base.result == 'success'", compute)
        self.assertIn("needs.build_candidate.result == 'success'", compute)
        self.assertIn("needs.build_base.outputs.artifact_id != ''", compute)
        self.assertIn("needs.build_candidate.outputs.artifact_id != ''", compute)

        for job in (build_base, build_candidate):
            self.assertIn('toolchain: "1.96"', job)
            self.assertEqual(
                job.count(
                    "cargo build --locked --release -p solar-compiler --bin solar"
                ),
                1,
            )
            self.assertIn('mkdir -p "$RUNNER_TEMP/lsp-bench-bin"', job)
            self.assertIn("runs-on: ubuntu-latest", job)
            self.assertNotIn("depot-ubuntu-latest", job)
        self.assertEqual(
            WORKFLOW.count(
                "cargo build --locked --release -p solar-compiler --bin solar"
            ),
            2,
        )
        self.assertIn(
            'CARGO_TARGET_DIR="$RUNNER_TEMP/lsp-bench-target/base"', build_base
        )
        self.assertIn(
            'CARGO_TARGET_DIR="$RUNNER_TEMP/lsp-bench-target/candidate"',
            build_candidate,
        )
        self.assertEqual(
            compute.count(
                "cargo build --locked --release \\\n"
                '              --manifest-path "$source_dir/Cargo.toml" --bin lsp-bench'
            ),
            1,
        )
        self.assertIn('toolchain: "1.96"', compute)
        self.assertEqual(compute.count("name: Run LSP comparison"), 1)
        self.assertNotIn("releases/download/v0.3.3/", compute)
        self.assertIn(source_url, compute)
        self.assertIn(upstream["commit"], compute)
        self.assertIn(upstream["source"]["sha256"], compute)
        self.assertIn(upstream["adapter"]["sha256"], compute)
        self.assertEqual(
            hashlib.sha256(adapter_path.read_bytes()).hexdigest(),
            upstream["adapter"]["sha256"],
        )
        self.assertIn(upstream["adapter"]["path"], compute)
        self.assertIn(version, compute)
        self.assertIn('CARGO_HOME="$RUNNER_TEMP/lsp-bench-cargo"', compute)
        self.assertIn(
            'CARGO_TARGET_DIR="$RUNNER_TEMP/lsp-bench-target/adapter"', compute
        )
        self.assertIn('patch --batch --forward --directory="$source_dir"', compute)
        self.assertIn('--lsp-bench "$RUNNER_TEMP/lsp-bench-tool/lsp-bench"', compute)
        self.assertNotIn("lsp_filter.py", WORKFLOW)
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

    def test_cross_server_stack_coexists_with_command_gate(self) -> None:
        workflow_names = {
            path.name for path in (ROOT / ".github/workflows").glob("lsp-bench*.yml")
        }

        self.assertEqual(
            workflow_names, {"lsp-bench-command.yml", "lsp-bench.yml"}
        )
        self.assertTrue((ROOT / "tools/lsp-bench/Cargo.toml").is_file())
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
