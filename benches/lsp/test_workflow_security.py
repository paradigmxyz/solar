from __future__ import annotations

import re
import unittest
from pathlib import Path

WORKFLOWS = Path(__file__).resolve().parents[2] / ".github" / "workflows"
EXECUTION_JOBS = {
    "lsp-bench-command.yml": ("build_base", "build_candidate", "compute"),
    "lsp-bench-cross-server-command.yml": ("benchmark",),
}


class WorkflowSecurityTests(unittest.TestCase):
    def test_benchmark_execution_is_read_only(self) -> None:
        for filename, execution_jobs in EXECUTION_JOBS.items():
            workflow = (WORKFLOWS / filename).read_text()
            self.assertRegex(workflow, r"(?m)^permissions: \{\}$")
            jobs = dict(
                re.findall(r"(?ms)^  ([a-z_]+):\n(.*?)(?=^  [a-z_]+:\n|\Z)", workflow)
            )
            for job_name in execution_jobs:
                with self.subTest(workflow=filename, job=job_name):
                    job = jobs[job_name]
                    permissions = re.search(
                        r"(?m)^    permissions:\n((?:^      [^\n]+\n)+)", job
                    )
                    self.assertIsNotNone(permissions)
                    assert permissions is not None
                    self.assertEqual(permissions[1], "      contents: read\n")
                    self.assertNotIn("secrets.", job)
                    self.assertNotIn("secrets[", job)
                    self.assertNotIn("actions/secure-runner@", job)
                    self.assertRegex(
                        job,
                        r"(?m)^        uses: tempoxyz/gh-actions/vendor/"
                        r"step-security/harden-runner@[0-9a-f]{40}[^\n]*\n"
                        r"        with:\n          egress-policy: audit\n",
                    )


if __name__ == "__main__":
    unittest.main()
