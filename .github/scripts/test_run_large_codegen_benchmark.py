import importlib.util
import json
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("run_large_codegen_benchmark.py")
SPEC = importlib.util.spec_from_file_location("large_codegen_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class ProjectSliceTests(unittest.TestCase):
    def test_resolves_relative_and_remapped_imports(self) -> None:
        project = {
            "sources": {
                "src/A.sol": {
                    "content": (
                        'import "./B.sol";\n'
                        'import {C} from "@pkg/C.sol";\n'
                    )
                },
                "src/B.sol": {"content": 'import "../shared/D.sol";'},
                "vendor/pkg/C.sol": {"content": ""},
                "shared/D.sol": {"content": ""},
                "unused.sol": {"content": ""},
            },
            "settings": {"remappings": ["@pkg/=vendor/pkg/"]},
        }

        selected = benchmark.project_slice(project, "src/A.sol")

        self.assertEqual(
            list(selected),
            ["shared/D.sol", "src/A.sol", "src/B.sol", "vendor/pkg/C.sol"],
        )

    def test_large_cases_resolve_from_pinned_projects(self) -> None:
        root = SCRIPT.parents[2]
        counts = {}
        for case in benchmark.CASES:
            project = json.loads((root / case.project).read_text())
            counts[case.test_id] = len(
                benchmark.project_slice(project, case.source)
            )

        self.assertEqual(
            counts,
            {
                "openzeppelin-governor": 48,
                "solady-signature-checker": 12,
                "solady-lib-string": 9,
            },
        )


class RuntimeComparisonTests(unittest.TestCase):
    def test_reports_cross_compiler_mismatch(self) -> None:
        entry = {
            "compilers": {
                "solc": {
                    "runtime_status": "ok",
                    "runtime_results": [
                        {"label": "value", "status": "ok", "value": "1"}
                    ],
                },
                "solar": {
                    "runtime_status": "ok",
                    "runtime_results": [
                        {"label": "value", "status": "ok", "value": "2"}
                    ],
                },
            }
        }

        benchmark.compare_runtime(entry, ("solc", "solar"))

        self.assertEqual(entry["runtime_status"], "mismatch")
        self.assertEqual(
            entry["runtime_mismatches"],
            [{"label": "value", "values": {"solc": "1", "solar": "2"}}],
        )


if __name__ == "__main__":
    unittest.main()
