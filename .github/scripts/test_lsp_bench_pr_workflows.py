#!/usr/bin/env python3

import copy
import json
import re
import shutil
import subprocess
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
PR_WORKFLOW = (ROOT / ".github/workflows/lsp-bench-pr.yml").read_text(encoding="utf-8")
COMMENT_WORKFLOW = (ROOT / ".github/workflows/lsp-bench-pr-comment.yml").read_text(
    encoding="utf-8"
)
PUBLISH_WORKFLOW = (ROOT / ".github/workflows/lsp-bench.yml").read_text(encoding="utf-8")
PROBE_WORKFLOW = (ROOT / ".github/workflows/lsp-bench-probe.yml").read_text(encoding="utf-8")
AUTHORITATIVE_WORKFLOW = (
    ROOT / ".github/workflows/lsp-bench-authoritative.yml"
).read_text(encoding="utf-8")
AUTHORITATIVE_VALIDATOR = (
    ROOT / ".github/scripts/lsp-bench-authoritative-validate.jq"
).read_text(encoding="utf-8")


class LspBenchPrWorkflowTests(unittest.TestCase):
    def test_candidate_checkout_and_provenance_use_the_pr_head(self) -> None:
        self.assertGreaterEqual(PR_WORKFLOW.count("github.event.pull_request.head.sha"), 4)
        self.assertIn("name: Verify PR head repository", PR_WORKFLOW)
        self.assertIn('run: test -n "$HEAD_REPOSITORY"', PR_WORKFLOW)
        self.assertIn(
            "github.event.pull_request.head.repo.full_name || github.repository",
            PR_WORKFLOW,
        )
        self.assertIn('test "$(git rev-parse HEAD)" = "$BENCHMARK_REVISION"', PR_WORKFLOW)
        self.assertIn('--revision "$BENCHMARK_REVISION"', PR_WORKFLOW)
        manifest_step = PR_WORKFLOW.split(
            "      - name: Generate PR benchmark manifest\n", 1
        )[1].split("\n      - name: ", 1)[0]
        self.assertIn("BENCHMARK_SOURCE_URL:", manifest_step)
        self.assertIn('--source-url "$BENCHMARK_SOURCE_URL"', manifest_step)
        self.assertIn("persist-credentials: false", PR_WORKFLOW)

    def test_only_an_exact_successful_main_run_can_supply_the_baseline(self) -> None:
        for contract in (
            "github.event.pull_request.base.ref == 'main'",
            'workflow_id: "lsp-bench-pr.yml"',
            'event: "push"',
            'status: "success"',
            "head_sha: process.env.BASE_SHA",
            'run.head_repository?.full_name === context.payload.repository.full_name',
            'run.head_branch === process.env.BASE_BRANCH',
            'run.head_sha === process.env.BASE_SHA',
            'run.conclusion === "success"',
            'if (matches.length !== 1)',
            'artifact.name === "lsp-bench-pr-baseline" && !artifact.expired',
            "steps.baseline-check.outcome == 'success'",
        ):
            self.assertIn(contract, PR_WORKFLOW)

    def test_commenter_uses_an_immutable_baseline_artifact(self) -> None:
        for contract in (
            'id: baseline-artifact',
            'artifact.name === "lsp-bench-pr-baseline-used" && !artifact.expired',
            'id: baseline-download',
            'BASELINE_DOWNLOAD_OUTCOME',
            'target/lsp-bench-comment/baseline/summary.json',
        ):
            self.assertIn(contract, COMMENT_WORKFLOW)
        self.assertIn('name: lsp-bench-pr-baseline-used', PR_WORKFLOW)
        self.assertIn(
            '"$BASELINE_DOWNLOAD_OUTCOME" == success', COMMENT_WORKFLOW
        )
        self.assertIn("steps.baseline-validate.outcome == 'success'", PR_WORKFLOW)
        self.assertIn("steps.baseline-benchmark.outcome == 'success'", PR_WORKFLOW)

    def test_baseline_artifacts_bind_the_executed_solar_binary(self) -> None:
        self.assertEqual(PR_WORKFLOW.count('$solar[0].status == "available"'), 2)
        self.assertEqual(
            PR_WORKFLOW.count(
                "$solar[0].artifact_expected_sha256\n"
                "                == $solar[0].executable_sha256"
            ),
            2,
        )
        self.assertEqual(
            PR_WORKFLOW.count(
                "$solar[0].artifact_sha256\n"
                "                == $solar[0].executable_sha256"
            ),
            2,
        )

    def test_baseline_used_artifact_is_uploaded_once_per_path(self) -> None:
        upload_steps = [
            block
            for block in PR_WORKFLOW.split("      - name: ")
            if "name: lsp-bench-pr-baseline-used" in block
        ]
        self.assertEqual(len(upload_steps), 2)
        self.assertIn("steps.baseline-validate.outcome == 'success'", upload_steps[0])
        self.assertIn("steps.baseline-validate.outcome != 'success'", upload_steps[1])
        self.assertIn("steps.baseline-benchmark.outcome == 'success'", upload_steps[1])

    def test_missing_or_invalid_artifacts_rebuild_the_exact_base(self) -> None:
        self.assertGreaterEqual(
            PR_WORKFLOW.count("steps.baseline-validate.outcome != 'success'"), 4
        )
        self.assertIn("ref: ${{ github.event.pull_request.base.sha }}", PR_WORKFLOW)
        self.assertIn(
            "ref: ${{ github.event.pull_request.base.sha }}\n"
            "          path: target/lsp-bench/base-source\n"
            "          fetch-depth: 0",
            PR_WORKFLOW,
        )
        self.assertIn('test "$(git rev-parse HEAD)" = "$BASE_SHA"', PR_WORKFLOW)
        self.assertIn('--revision "${{ github.event.pull_request.base.sha }}"', PR_WORKFLOW)

    def test_fallback_contract_drift_is_fixed_inconclusive(self) -> None:
        for contract in (
            "id: fallback-contract",
            "compare_file tools/lsp-bench/benchmark.yaml",
            "compare_file tools/lsp-bench/servers.lock.yaml",
            "compare_file tools/lsp-bench/fixtures.lock.yaml",
            "compare_file .github/scripts/lsp-bench-pr-config.py",
            "compare_file tools/lsp-bench/Cargo.toml",
            "compare_file Cargo.toml",
            "compare_file Cargo.lock",
            "compare_tracked_tree tools/lsp-bench/src",
            'git -C "$root" diff --quiet -- "$relative"',
            'git -C "$root" ls-files --others -- "$relative"',
            'check_clean_tree "$GITHUB_WORKSPACE" candidate tools/lsp-bench/src',
            'check_clean_tree "$BASE_SOURCE" base tools/lsp-bench/src',
            'check_clean_tree "$GITHUB_WORKSPACE" candidate tools/lsp-bench/fixtures',
            'check_clean_tree "$BASE_SOURCE" base tools/lsp-bench/fixtures',
            "git -C \"$GITHUB_WORKSPACE\" ls-files --stage -- tools/lsp-bench/fixtures",
            "git -C \"$BASE_SOURCE\" ls-files --stage -- tools/lsp-bench/fixtures",
            'echo "match=false" >> "$GITHUB_OUTPUT"',
            (
                "The benchmark contract changed in this PR, so the exact base fallback "
                "was not compared."
            ),
            "steps.fallback-contract.outputs.match == 'true'",
        ):
            self.assertIn(contract, PR_WORKFLOW)

    def test_pr_updates_cancel_but_main_pushes_do_not(self) -> None:
        self.assertRegex(
            PR_WORKFLOW,
            re.compile(
                r"github\.event_name == 'pull_request' && "
                r"github\.event\.pull_request\.number \|\| github\.run_id"
            ),
        )
        self.assertIn(
            "cancel-in-progress: ${{ github.event_name == 'pull_request' }}", PR_WORKFLOW
        )

    def test_privileged_commenter_never_checks_out_pr_code(self) -> None:
        self.assertEqual(COMMENT_WORKFLOW.count("uses: actions/checkout@"), 1)
        self.assertIn("repository: ${{ github.repository }}", COMMENT_WORKFLOW)
        self.assertIn("ref: ${{ github.workflow_sha }}", COMMENT_WORKFLOW)
        self.assertIn("path: target/lsp-bench-comment/trusted-source", COMMENT_WORKFLOW)
        self.assertIn("persist-credentials: false", COMMENT_WORKFLOW)
        self.assertIn("issues: write", COMMENT_WORKFLOW)
        self.assertIn("pull-requests: write", COMMENT_WORKFLOW)
        self.assertIn("actions: read", COMMENT_WORKFLOW)
        self.assertIn(
            'const workflowPath = ".github/workflows/lsp-bench-pr.yml"', COMMENT_WORKFLOW
        )
        self.assertIn(
            "run.repository?.full_name !== context.payload.repository.full_name",
            COMMENT_WORKFLOW,
        )
        self.assertIn(
            "const head = `${run.head_repository.owner.login}:${run.head_branch}`",
            COMMENT_WORKFLOW,
        )
        self.assertIn("pull.head.sha !== run.head_sha", COMMENT_WORKFLOW)
        self.assertIn("associated.head.repo?.id !== pull.head.repo?.id", COMMENT_WORKFLOW)
        self.assertIn("pull.base.sha !== process.env.BASE_SHA", COMMENT_WORKFLOW)
        self.assertIn(
            'core.setOutput("base_source_url", pull.base.repo.clone_url)',
            COMMENT_WORKFLOW,
        )
        self.assertIn(
            'core.setOutput("head_source_url", pull.head.repo.clone_url)',
            COMMENT_WORKFLOW,
        )
        self.assertIn(
            '--expected-baseline-source-url "$BASE_SOURCE_URL"', COMMENT_WORKFLOW
        )
        self.assertIn(
            '--expected-candidate-source-url "$HEAD_SOURCE_URL"', COMMENT_WORKFLOW
        )
        self.assertIn(
            'artifact.name === "lsp-bench-pr-results" && !artifact.expired',
            COMMENT_WORKFLOW,
        )
        self.assertIn("if (matches.length !== 1)", COMMENT_WORKFLOW)
        self.assertIn("artifact-ids: ${{ steps.artifact.outputs.artifact_id }}", COMMENT_WORKFLOW)
        self.assertIn(
            "python3 target/lsp-bench-comment/trusted-source/.github/scripts/",
            COMMENT_WORKFLOW,
        )

    def test_full_workflows_use_github_hosted_ubuntu_without_registration(self) -> None:
        branch_guard = (
            "if: github.ref == format('refs/heads/{0}', "
            "github.event.repository.default_branch)"
        )
        for workflow in (PUBLISH_WORKFLOW, PROBE_WORKFLOW):
            self.assertIn(branch_guard, workflow)
            self.assertIn("persist-credentials: false", workflow)
            self.assertIn("runs-on: ubuntu-24.04", workflow)
            self.assertNotIn("self-hosted", workflow)
        self.assertIn("name: cross-server comparison (GitHub-hosted)", PUBLISH_WORKFLOW)
        self.assertIn("default: smoke", PUBLISH_WORKFLOW)
        self.assertIn("default: core", PUBLISH_WORKFLOW)
        self.assertIn("BENCHMARK_PROFILE: ${{ inputs.profile }}", PUBLISH_WORKFLOW)
        self.assertIn("BENCHMARK_SCOPE: ${{ inputs.scope }}", PUBLISH_WORKFLOW)
        self.assertGreaterEqual(
            PUBLISH_WORKFLOW.count("core) server_args=(--server solar --server asyncswap)"),
            4,
        )
        self.assertIn('--profile "$BENCHMARK_PROFILE"', PUBLISH_WORKFLOW)
        self.assertIn('"${server_args[@]}"', PUBLISH_WORKFLOW)
        self.assertIn("solar-lsp-bench validate-results", PUBLISH_WORKFLOW)
        self.assertIn('echo "benchmark_scope=$BENCHMARK_SCOPE"', PUBLISH_WORKFLOW)
        self.assertIn('if [[ "$BENCHMARK_SCOPE" == all ]]', PUBLISH_WORKFLOW)
        self.assertNotIn("--profile default", PUBLISH_WORKFLOW)
        self.assertIn('>> "$GITHUB_STEP_SUMMARY"', PUBLISH_WORKFLOW)
        self.assertIn("cat target/lsp-bench/publish/COMPARISON.md", PUBLISH_WORKFLOW)
        self.assertIn(
            "solidity-lsp-benchmark-${{ inputs.profile }}-${{ inputs.scope }}-${{ github.run_id }}",
            PUBLISH_WORKFLOW,
        )
        self.assertNotIn("doctor --publish", PUBLISH_WORKFLOW)
        self.assertNotIn("--require-authoritative", PUBLISH_WORKFLOW)
        self.assertIn("continue-on-error: true", PUBLISH_WORKFLOW)
        self.assertIn("continue-on-error: true", PROBE_WORKFLOW)

    def test_authoritative_workflow_is_strict_and_self_hosted(self) -> None:
        self.assertIn("runs-on: [self-hosted, linux, x64, lsp-bench]", AUTHORITATIVE_WORKFLOW)
        self.assertIn("run: .github/scripts/lsp-bench-probe.sh", AUTHORITATIVE_WORKFLOW)
        self.assertIn("doctor --publish", AUTHORITATIVE_WORKFLOW)
        self.assertIn("--profile publish", AUTHORITATIVE_WORKFLOW)
        self.assertIn("solar-lsp-bench validate-results", AUTHORITATIVE_WORKFLOW)
        self.assertIn("jq -e -f .github/scripts/lsp-bench-authoritative-validate.jq", AUTHORITATIVE_WORKFLOW)
        self.assertIn(".environment.authoritative == true", AUTHORITATIVE_VALIDATOR)
        self.assertIn("$summary_keys | sort == ($expected_keys | sort)", AUTHORITATIVE_VALIDATOR)
        self.assertIn("($metadata.content_sha256 | sha256)", AUTHORITATIVE_VALIDATOR)
        self.assertIn("retention-days: 90", AUTHORITATIVE_WORKFLOW)
        self.assertNotIn("retention-days: 365", AUTHORITATIVE_WORKFLOW)
        self.assertIn(".successful_runs == $workloads[.workload].repetitions", AUTHORITATIVE_VALIDATOR)
        self.assertIn("(.status_counts | keys) == [\"pass\"]", AUTHORITATIVE_VALIDATOR)
        self.assertIn("artifact-manifest.sha256", AUTHORITATIVE_WORKFLOW)
        self.assertIn("--require-authoritative", AUTHORITATIVE_WORKFLOW)
        self.assertIn("$workload_ids | length > 0 and length == (unique | length)", AUTHORITATIVE_VALIDATOR)
        self.assertIn("$summary_keys | sort == ($expected_keys | sort)", AUTHORITATIVE_VALIDATOR)
        self.assertIn("($metadata.content_sha256 | sha256)", AUTHORITATIVE_VALIDATOR)

    def test_authoritative_manifest_covers_inputs_provenance_and_final_results(self) -> None:
        manifest_step = AUTHORITATIVE_WORKFLOW.split(
            "      - name: Write immutable artifact manifest\n", 1
        )[1].split("\n      - name: ", 1)[0]
        for path in (
            "target/lsp-bench/authoritative/samples.json",
            "target/lsp-bench/authoritative/samples.jsonl",
            "target/lsp-bench/authoritative/summary.json",
            "target/lsp-bench/authoritative/summary.md",
            "target/lsp-bench/authoritative/COMPARISON.md",
        ):
            self.assertIn(path, manifest_step)
        self.assertIn("target/lsp-bench/provenance", manifest_step)
        self.assertIn("tools/lsp-bench/install", manifest_step)
        self.assertIn("export LC_ALL=C", manifest_step)
        self.assertIn("tools/lsp-bench/install/", AUTHORITATIVE_WORKFLOW)
        self.assertIn("target/lsp-bench/provenance/", AUTHORITATIVE_WORKFLOW)

    def test_authoritative_summary_validator_checks_matrix_and_status(self) -> None:
        jq = shutil.which("jq")
        if jq is None:
            self.skipTest("jq is unavailable")
        digest = "a" * 64
        executable_digest = "b" * 64
        solc_digest = "c" * 64
        soljson_digest = "d" * 64
        foundry_digest = "e" * 64
        revision = "f" * 40
        summary = {
            "schema_version": 5,
            "config_schema_version": 1,
            "config_sha256": digest,
            "servers_lock_sha256": digest,
            "fixtures_lock_sha256": digest,
            "harness_git_revision": revision,
            "harness_git_dirty": False,
            "profile": "publish",
            "repeat_override": None,
            "environment": {"authoritative": True, "network_isolated": True},
            "servers": [
                {
                    "id": "server",
                    "status": "available",
                    "version": "server 1.2.3",
                    "locked_version": "1.2.3",
                    "source": {
                        "url": "https://example.invalid/server.git",
                        "revision": revision,
                    },
                    "executable_sha256": executable_digest,
                    "artifact_expected_sha256": digest,
                    "artifact_sha256": digest,
                }
            ],
            "fixtures": [
                {
                    "id": "fixture",
                    "revision": revision,
                    "source": {
                        "url": "https://example.invalid/fixture.git",
                        "revision": revision,
                    },
                    "content_sha256": digest,
                    "source_file_count": 1,
                    "solc": {
                        "version": "0.8.36",
                        "native": "/opt/solc",
                        "native_sha256": solc_digest,
                        "soljson": "/opt/soljson.js",
                        "soljson_sha256": soljson_digest,
                    },
                    "solc_native_sha256": solc_digest,
                    "solc_soljson_sha256": soljson_digest,
                    "solc_native_version": "Version: 0.8.36+commit.8a079791",
                    "foundry": {
                        "version": "1.7.1",
                        "native": "/opt/forge",
                        "native_sha256": foundry_digest,
                        "archive_sha256": digest,
                    },
                    "foundry_native_sha256": foundry_digest,
                    "foundry_native_version": "forge Version: 1.7.1-stable",
                }
            ],
            "workloads": [{"id": "workload", "fixture": "fixture", "repetitions": 2}],
            "summaries": [
                {
                    "server": "server",
                    "workload": "workload",
                    "fixture": "fixture",
                    "status": "pass",
                    "successful_runs": 2,
                    "status_counts": {"pass": 2},
                }
            ],
        }
        summary_path = ROOT / "target/lsp-bench-authoritative-validator-test.json"
        validator = ROOT / ".github/scripts/lsp-bench-authoritative-validate.jq"

        def validate(value: object) -> subprocess.CompletedProcess[str]:
            summary_path.write_text(json.dumps(value), encoding="utf-8")
            return subprocess.run(
                [jq, "-e", "-f", str(validator), str(summary_path)],
                capture_output=True,
                text=True,
            )

        try:
            summary_path.parent.mkdir(parents=True, exist_ok=True)
            result = validate(summary)
            self.assertEqual(result.returncode, 0)

            invalid_evidence = (
                ("config digest", ("config_sha256",), "unavailable"),
                ("server lock digest", ("servers_lock_sha256",), None),
                ("fixture lock digest", ("fixtures_lock_sha256",), None),
                ("harness revision", ("harness_git_revision",), "short"),
                ("server observed version", ("servers", 0, "version"), None),
                (
                    "server observed version token",
                    ("servers", 0, "version"),
                    "server 11.2.30",
                ),
                ("server source revision", ("servers", 0, "source", "revision"), "short"),
                ("server executable digest", ("servers", 0, "executable_sha256"), None),
                ("server artifact digest", ("servers", 0, "artifact_sha256"), "0" * 64),
                ("fixture revision", ("fixtures", 0, "revision"), "short"),
                ("fixture content digest", ("fixtures", 0, "content_sha256"), "unavailable"),
                ("solc native digest", ("fixtures", 0, "solc_native_sha256"), "0" * 64),
                ("solc version", ("fixtures", 0, "solc_native_version"), None),
                (
                    "solc observed version token",
                    ("fixtures", 0, "solc_native_version"),
                    "Version: 10.8.360",
                ),
                ("soljson digest", ("fixtures", 0, "solc_soljson_sha256"), "0" * 64),
                ("Foundry native digest", ("fixtures", 0, "foundry_native_sha256"), "0" * 64),
                ("Foundry version", ("fixtures", 0, "foundry_native_version"), None),
                ("summary status", ("summaries", 0, "status"), "failed"),
            )
            for name, path, value in invalid_evidence:
                with self.subTest(name=name):
                    invalid = copy.deepcopy(summary)
                    parent = invalid
                    for component in path[:-1]:
                        parent = parent[component]
                    parent[path[-1]] = value
                    self.assertNotEqual(validate(invalid).returncode, 0)

            source_build = copy.deepcopy(summary)
            source_build["servers"][0]["artifact_expected_sha256"] = None
            source_build["servers"][0]["artifact_sha256"] = executable_digest
            self.assertEqual(validate(source_build).returncode, 0)
            source_build["servers"][0]["artifact_sha256"] = digest
            self.assertNotEqual(validate(source_build).returncode, 0)

            synthetic_fixture = copy.deepcopy(summary)
            synthetic_fixture["fixtures"][0]["source"] = None
            synthetic_fixture["fixtures"][0]["revision"] = None
            self.assertEqual(validate(synthetic_fixture).returncode, 0)
        finally:
            summary_path.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
