"""The Lean namespace of a contract's exported stages (solar -Zemit-lean-stages).

solar puts a contract's stage definitions under `-Zlean-stages-namespace` (by default
SolarTV.Export), then the contract's name, and records the namespace in the export's
manifest.json. The driver exports each run under its own namespace, SolarTV.Export.<run name>,
so a published statement whose library imports reach another export of a contract with the same
name (solidity-lean's own certified examples define SolarTV.Export.Main, .Events and .Bank) still
elaborates. An export without a recorded namespace (an older solar, or no flag) is under the
default one.
"""

import json
import pathlib
import re

DEFAULT = "SolarTV.Export"
IDENT = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")


def contract_ident(contract):
    """The contract's name as the exporter writes it in a namespace."""
    plain = re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", contract)
    return contract if plain else f"«{contract}»"


def of_contract(contract, prefix=DEFAULT):
    """The namespace of CONTRACT's definitions under PREFIX."""
    return f"{prefix}.{contract_ident(contract)}"


def run_prefix(name):
    """The prefix the driver exports run NAME under (passed as -Zlean-stages-namespace)."""
    if not IDENT.fullmatch(name):
        raise ValueError(f"run name {name!r} is not a Lean identifier")
    return f"{DEFAULT}.{name}"


def of_run(name, contract):
    """The namespace of CONTRACT's definitions in run NAME's export."""
    return of_contract(contract, run_prefix(name))


def of_gen(gen):
    """The namespace of an export directory: the one its manifest.json records, else (an export
    without a manifest, such as a test's copy of some stages) the namespace its Bytecode.lean or
    first stage file opens."""
    gen = pathlib.Path(gen)
    if (gen / "manifest.json").exists():
        manifest = json.loads((gen / "manifest.json").read_text())
        return manifest.get("namespace") or of_contract(manifest["contract"])
    for f in [gen / "Bytecode.lean", *sorted(gen.glob("*.lean"))]:
        m = f.exists() and re.search(r"^namespace (\S+)$", f.read_text(), re.M)
        if m:
            return m.group(1)
    raise ValueError(f"{gen}: no manifest.json and no stage file with a namespace")


def of_shape(shape, contract):
    """The namespace a shape.json names (`export_ns`), or the default one for CONTRACT."""
    return shape.get("export_ns") or of_contract(contract)
