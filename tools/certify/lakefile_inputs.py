"""The one rule for lakefile.lean in a run's inputs commit (scripts/certify through checkout.sh, and
generic.py).

The inputs commit may change lakefile.lean only by appending the driver's canonical library stanza,
lakefile_lib(NAME, CONTRACT), byte for byte at the end of the base's lakefile, for a NAME the base's
lakefile does not declare. The tracked file must equal the base's bytes followed by exactly that
stanza: no other edit, no second stanza, no stanza for another name, and package options, imports
and roots stay byte-identical. The only free parameter is CONTRACT, which must be a Solidity
identifier; the whole appended text is then compared as bytes with the stanza rendered for it.

NAME must be in the driver's name grammar (generic.py's main: the source's lowercased stem with every
other character as _, then _ and the first 8 hex digits of its sha256), so a name with a newline, a
quote, a guillemet, a path separator or .. never reaches a stanza.

  python3 -I lakefile_inputs.py check REPO BASE HEAD INPUTS_DIR [CONTRACT]

exits 0 when HEAD's lakefile.lean is BASE's or BASE's plus the stanza for the NAME of INPUTS_DIR
(Certified/NAME), and 1 naming why not.
"""

import re
import subprocess
import sys

NAME_RE = re.compile(r"[a-z][a-z0-9_]*_[0-9a-f]{8}")
CONTRACT_RE = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")
INPUTS_PREFIX = "Certified/"


class Refused(Exception):
    pass


def check_name(name):
    if not (isinstance(name, str) and NAME_RE.fullmatch(name)):
        raise Refused(
            f"{name!r} is not a name the driver gives a run ([a-z][a-z0-9_]*_ + 8 hex)"
        )


def check_contract(contract):
    if not (isinstance(contract, str) and CONTRACT_RE.fullmatch(contract)):
        raise Refused(f"{contract!r} is not a Solidity contract name")


def lean_lib_body(name):
    """The declaration of the stanza, without its comment."""
    check_name(name)
    return f"""lean_lib «{name}» where
  srcDir := "Certified"
  roots := #[§{name}.Proofs]
  globs := #[.submodules §{name}]
""".replace("§", "\u0060")


def lakefile_lib(name, contract):
    """The library stanza the driver appends for Certified/NAME."""
    check_name(name)
    check_contract(contract)
    return f"""
-- {name}: the stages of Certified/{name}/{name}.sol's {contract} and their kernel proofs, which
-- solar's scripts/certify writes.
""" + lean_lib_body(name)


def lean_lib_declared(text, name):
    """Whether the lakefile TEXT declares lean_lib NAME, as `lean_lib «NAME»` or `lean_lib NAME`."""
    return (
        re.search(
            rf"^lean_lib («{re.escape(name)}»|{re.escape(name)})( |$)",
            text,
            re.MULTILINE,
        )
        is not None
    )


def problem(base, head, name, contract=None):
    """Why the lakefile bytes HEAD are not the lakefile bytes BASE, or BASE plus lakefile_lib(NAME, C)
    for a NAME BASE does not declare (C = CONTRACT when given), or None."""
    if head == base:
        return None
    try:
        check_name(name)
        if contract is not None:
            check_contract(contract)
    except Refused as e:
        return str(e)
    try:
        base_text = base.decode("utf-8")
    except UnicodeDecodeError:
        return "the base's lakefile.lean is not UTF-8"
    if lean_lib_declared(base_text, name):
        return f"the base's lakefile already declares lean_lib {name}"
    if not head.startswith(base):
        return "it edits the base's lakefile.lean, not only appends to it"
    added = head[len(base) :]
    # The stanza's one free parameter, read back only to render the stanza it must equal.
    sentinel = "\x00"
    pre, post = (
        lakefile_lib(name, "C")
        .replace("'s C and", "'s " + sentinel + " and")
        .split(sentinel)
    )
    pre_b, post_b = pre.encode(), post.encode()
    if not (
        added.startswith(pre_b)
        and added.endswith(post_b)
        and len(added) > len(pre_b) + len(post_b)
    ):
        return f"what it appends is not the library stanza for {name}"
    try:
        c = added[len(pre_b) : len(added) - len(post_b)].decode("utf-8")
        check_contract(c)
    except UnicodeDecodeError, Refused:
        return f"what it appends is not the library stanza for {name}"
    if contract is not None and c != contract:
        return f"the stanza names contract {c!r}, not {contract!r}"
    if added != lakefile_lib(name, c).encode():
        return f"what it appends is not the library stanza for {name}"
    return None


def name_of(inputs_dir):
    """NAME of the inputs directory Certified/NAME, or None."""
    if not inputs_dir.startswith(INPUTS_PREFIX):
        return None
    n = inputs_dir[len(INPUTS_PREFIX) :]
    return n if NAME_RE.fullmatch(n) else None


def show(repo, rev):
    p = subprocess.run(
        ["git", "-C", repo, "show", f"{rev}:lakefile.lean"],
        capture_output=True,
        check=False,
    )
    if p.returncode != 0:
        raise Refused(f"cannot read {rev}:lakefile.lean")
    return p.stdout


def main(argv):
    if len(argv) not in (5, 6) or argv[0] != "check":
        print(
            "usage: lakefile_inputs.py check REPO BASE HEAD INPUTS_DIR [CONTRACT]",
            file=sys.stderr,
        )
        return 2
    repo, base, head, inputs_dir = argv[1:5]
    contract = argv[5] if len(argv) == 6 else None
    name = name_of(inputs_dir)
    if name is None:
        print(
            f"{inputs_dir!r} is not Certified/NAME for a name the driver gives a run",
            file=sys.stderr,
        )
        return 1
    try:
        why = problem(show(repo, base), show(repo, head), name, contract)
    except Refused as e:
        why = str(e)
    if why is not None:
        print(why, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
