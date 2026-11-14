"""Whole-entry memory is not restricted to the local fragment's 256-byte projection."""

import unittest

import test_world as world


def program(copy):
    return (
        "push 17\npush 1024\nmstore\npush 170\npush 1057\nmstore8\n"
        + copy
        + "\npush 1088\nmload\npush 0\nmstore\n"
        "push 32\npush 1088\nkeccak256\npush 32\nmstore\n"
        "msize\npush 64\nmstore\npush 96\npush 0\nreturn"
    )


class WorldMemoryTests(unittest.TestCase):
    pair = world.WorldTests.pair

    def test_large_memory_copy_specialization_and_wrong_source(self):
        before = program("push 32\npush 1024\npush 1088\nmcopy")
        after = program("push 17\npush 1088\nmstore")
        _, record = self.pair(before, after, mode="refine")
        self.assertEqual(record["status"], "verified")
        self.pair(before, program("push 16\npush 1088\nmstore"), status=1)
