#!/usr/bin/env python3
"""export_ns.py: the namespace of a run's exported stages.
Run under the Python generic.py needs (3.14): python3.14 tools/certify/test_export_ns.py"""

import json
import pathlib
import sys
import tempfile
import unittest

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import export_ns


class Namespace(unittest.TestCase):
    def test_run_namespace(self):
        self.assertEqual(export_ns.of_run("c4_31c39cd4", "Main"), "SolarTV.Export.c4_31c39cd4.Main")
        self.assertEqual(export_ns.of_contract("A$b"), "SolarTV.Export.«A$b»")
        with self.assertRaises(ValueError):
            export_ns.run_prefix("1x")

    def test_manifest_records_it(self):
        with tempfile.TemporaryDirectory() as t:
            gen = pathlib.Path(t)
            (gen / "manifest.json").write_text(json.dumps({"contract": "C", "namespace": "R.x.C"}))
            self.assertEqual(export_ns.of_gen(gen), "R.x.C")
            (gen / "manifest.json").write_text(json.dumps({"contract": "C"}))
            self.assertEqual(export_ns.of_gen(gen), "SolarTV.Export.C")

    def test_stage_files_without_a_manifest(self):
        with tempfile.TemporaryDirectory() as t:
            gen = pathlib.Path(t)
            (gen / "Bytecode.lean").write_text("-- x\nnamespace SolarTV.Export.r_1.C\n\nend SolarTV.Export.r_1.C\n")
            self.assertEqual(export_ns.of_gen(gen), "SolarTV.Export.r_1.C")

    def test_shape_names_it(self):
        self.assertEqual(export_ns.of_shape({"export_ns": "A.B.C"}, "C"), "A.B.C")
        self.assertEqual(export_ns.of_shape({}, "C"), "SolarTV.Export.C")


if __name__ == "__main__":
    unittest.main()
