"""Resolve the source revision and label for a stacked PR benchmark baseline."""

import json
import re
import subprocess
import sys


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def resolve_base(base_sha: str, repository: str, pr_number: str) -> dict[str, str]:
    pull = json.loads(command("gh", "api", f"repos/{repository}/pulls/{pr_number}"))
    parent = pull["base"]
    message = command("git", "show", "-s", "--format=%B", base_sha)
    parents = command("git", "show", "-s", "--format=%P", base_sha).split()
    merge = re.fullmatch(r"Merge ([0-9a-f]{40}) into ([0-9a-f]{40})", message)
    # Native stacks can expose an unchanged synthetic merge as the tested base.
    # Resolve its immutable parent, even if the branch has since advanced.
    if (
        merge
        and parents == [merge[2], merge[1]]
        and command("git", "rev-parse", f"{base_sha}^{{tree}}")
        == command("git", "rev-parse", f"{parents[1]}^{{tree}}")
    ):
        base_sha = parents[1]
    return {
        "sha": base_sha,
        "ref": parent["ref"] if base_sha == parent["sha"] else base_sha[:8],
    }


if __name__ == "__main__":
    print(json.dumps(resolve_base(*sys.argv[1:])))
