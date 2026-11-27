"""Propose exact EQ/SUB branch inversions for the kernel to check."""

from collections import Counter


def propose(before, after):
    def locations(model):
        counts = Counter()
        keys = {}
        for row in model["nodes"]:
            label = row["label"]
            keys[label, counts[label]] = row["id"]
            counts[label] += 1
        return keys

    old = locations(before)
    new = locations(after)
    models = (before, after)
    sites = ([], [])
    for place in old.keys() & new.keys():
        points = old[place], new[place]
        operations = tuple(
            model["nodes"][pc]["node"] for model, pc in zip(models, points, strict=True)
        )
        names = tuple(node.get("operation") for node in operations)
        if set(names) != {"eq", "sub"} or not all(
            node["kind"] == "operation"
            and model["nodes"][node["next"]]["node"]["kind"] == "branch"
            for node, model in zip(operations, models, strict=True)
        ):
            continue
        side = names.index("sub")
        sites[side].append(points[side])

    result = []
    for model, comparisons in zip(models, sites, strict=True):
        comparisons.sort()
        if not comparisons:
            result.append(model)
            continue
        rows = [dict(row, node=dict(row["node"])) for row in model["nodes"]]
        for pc in comparisons:
            operation = rows[pc]["node"]
            branch = rows[operation["next"]]["node"]
            operation["operation"] = "eq"
            branch["yes"], branch["no"] = branch["no"], branch["yes"]
        result.append(dict(model, nodes=rows))
    return result, sites


def certificate(name, original, sites):
    from . import cfg_flow

    terms = ",".join(map(str, sites))
    return (
        cfg_flow.table(f"{name}OriginalNodes", "EVM.ContinuationFlow.Node", original)
        + f"""
def {name}Original : EVM.ContinuationFlow.Program :=
  ⟨{name}Source.entry,{name}OriginalNodes.rows.toArray⟩
def {name}Sites : List Nat := [{terms}]
theorem {name}Comparisons :
    EVM.BranchLayout.normalize {name}Original {name}Sites = some {name}Source :=
  EVM.BranchLayout.normalize_checked {name}Original {name}Source {name}OriginalNodes.view
    {name}Sites {name}OriginalNodesLookup rfl (by decide +kernel)
"""
    )
