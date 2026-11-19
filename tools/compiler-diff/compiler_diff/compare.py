"""Versioned artifact comparisons; never reorder positional ABI components."""

import copy
import json
import re
import unittest
from collections import Counter, defaultdict

from .artifacts import STORAGE_LAYOUTS, contract_outputs

VERSION = 1
STORAGE_LAYOUT_VERSION = 1
# Each comparator's rule version, which expectations must match.
COMPARATORS = {
    "abi": VERSION,
    "methods": VERSION,
    "userdoc": VERSION,
    "devdoc": VERSION,
    "ast": VERSION,
    "storage-layout": STORAGE_LAYOUT_VERSION,
}
# A user-defined type's kind, name and AST ID, as in `t_struct(S)12_storage`.
STORAGE_TYPE_ID = re.compile(
    r"(t_(?:struct|enum|contract|userDefinedValueType))(\([^)]*\))(\d+)"
)
# A type label such as `struct Tick.Info`; others could forge type ID syntax.
STORAGE_TYPE_LABEL = re.compile(r"[\w$. ]+")
# A user-defined type without an AST ID, which compilers never emit.
STORAGE_TYPE_WITHOUT_ID = re.compile(
    r"t_(?:struct|enum|contract|userDefinedValueType)\([^)]*\)(?!\d)"
)
STORAGE_TYPE_FIELDS = {"type", "key", "value", "base"}
AST_RULE_VERSION = 1
AST_SAMPLES = 20
# Fields holding node IDs, as one ID or a list of them. `exportedSymbols` maps
# names to lists, and `internalFunctionIDs` and `externalReferences` hold IDs in
# keys and entries; `translate` renames those too.
AST_ID_FIELDS = {
    "assignments",
    "baseFunctions",
    "baseModifiers",
    "contractDependencies",
    "declaration",
    "function",
    "functionReturnParameters",
    "id",
    "linearizedBaseContracts",
    "overloadedDeclarations",
    "referencedDeclaration",
    "scope",
    "sourceUnit",
    "usedErrors",
    "usedEvents",
}
# Solc sorts these sets by node ID, which carries no meaning here.
AST_UNORDERED_FIELDS = {
    "baseFunctions",
    "baseModifiers",
    "contractDependencies",
    "usedErrors",
    "usedEvents",
}
# The ID in `t_struct$_S_$12_storage_ptr` `t_type$_t_super$_B_$7_$` and `t_module_7`; array lengths also follow `_$`.
# Spans are `start:length:source index`; the index follows the compilation order.
AST_SPAN_FIELDS = {
    "keyNameLocation",
    "memberLocation",
    "nameLocation",
    "nameLocations",
    "nativeSrc",
    "src",
    "valueNameLocation",
}
AST_SPAN = re.compile(r"(\d+:\d+:)(\d+)")
AST_TYPE_ID = re.compile(
    r"(t_(?:struct|contract|enum|userDefinedValueType|super)\$_.*?_\$|t_module_)(\d+)"
)


def canonical_type(item):
    if not isinstance(item, dict):
        raise ValueError("ABI parameter must be an object")
    value = item.get("type")
    if not isinstance(value, str):
        raise ValueError("ABI type must be a string")
    if value.startswith("tuple"):
        if not isinstance(item.get("components"), list):
            raise ValueError("ABI components must be an array")
        return (
            "("
            + ",".join(canonical_type(c) for c in item["components"])
            + ")"
            + value[len("tuple") :]
        )
    return value


def pointer(path, key):
    return path + "/" + str(key).replace("~", "~0").replace("/", "~1")


def differences(left, right, path=""):
    if type(left) is not type(right):
        return [{"path": path, "kind": "value", "left": left, "right": right}]
    result = []
    if isinstance(left, dict):
        for key in sorted(left.keys() | right.keys()):
            location = pointer(path, key)
            if key not in left or key not in right:
                result.append(
                    {
                        "path": location,
                        "kind": "missing-left" if key not in left else "missing-right",
                        "left": left.get(key),
                        "right": right.get(key),
                    }
                )
            else:
                result.extend(differences(left[key], right[key], location))
    elif isinstance(left, list):
        for index in range(max(len(left), len(right))):
            if index >= len(left) or index >= len(right):
                result.append(
                    {
                        "path": pointer(path, index),
                        "kind": "missing-left"
                        if index >= len(left)
                        else "missing-right",
                        "left": left[index] if index < len(left) else None,
                        "right": right[index] if index < len(right) else None,
                    }
                )
            else:
                result.extend(
                    differences(left[index], right[index], pointer(path, index))
                )
    elif left != right:
        result.append({"path": path, "kind": "value", "left": left, "right": right})
    return result


def abi_entries(abi, policy):
    if not isinstance(abi, list):
        raise ValueError("ABI must be an array")

    def param(item, event=False):
        if not isinstance(item, dict):
            raise ValueError("ABI parameter must be an object")
        canonical_type(item)
        result = dict(item)
        if "components" in item:
            if not isinstance(item["components"], list):
                raise ValueError("ABI components must be an array")
            result["components"] = [param(c) for c in item["components"]]
        if event and not isinstance(item.get("indexed"), bool):
            raise ValueError("event parameter must have an indexed boolean")
        if policy == "interface":
            result.pop("name", None)
            result.pop("internalType", None)
        return result

    normalized = {}
    for entry in abi:
        if not isinstance(entry, dict):
            raise ValueError("ABI entry must be an object")
        kind = entry.get("type")
        if kind not in {
            "function",
            "event",
            "error",
            "constructor",
            "fallback",
            "receive",
        }:
            raise ValueError(f"unknown ABI entry type: {kind}")
        result = dict(entry)
        if kind in {"function", "event", "error", "constructor"}:
            if not isinstance(entry.get("inputs"), list):
                raise ValueError("ABI inputs must be an array")
            result["inputs"] = [param(p, kind == "event") for p in entry["inputs"]]
        if kind == "function":
            if not isinstance(entry.get("outputs"), list):
                raise ValueError("ABI outputs must be an array")
            result["outputs"] = [param(p) for p in entry["outputs"]]
        if kind in {"function", "constructor", "fallback", "receive"} and entry.get(
            "stateMutability"
        ) not in {"pure", "view", "nonpayable", "payable"}:
            raise ValueError("invalid ABI stateMutability")
        if kind == "event" and not isinstance(entry.get("anonymous"), bool):
            raise ValueError("event must have an anonymous boolean")
        key = kind
        if kind in {"function", "event", "error"}:
            if not isinstance(entry.get("name"), str):
                raise ValueError("ABI entry must have a name")
            key += (
                ":"
                + entry["name"]
                + "("
                + ",".join(canonical_type(p) for p in entry["inputs"])
                + ")"
            )
        normalized.setdefault(key, []).append(result)
    # Sort only ABI entries; preserve duplicate entries and positional arrays.
    return {
        key: sorted(entries, key=lambda e: json.dumps(e, sort_keys=True))
        for key, entries in normalized.items()
    }


def storage_layouts(layouts, where):
    """Drop AST IDs from a contract's storage layouts, keyed by output name.

    `t_struct(Info)3` becomes `t_struct(Info){struct Tick.Info}`, with the label
    of the `types` entry that the ID heads, so same-named types from different
    scopes stay apart. An ID with no entry keeps its bare name,
    `t_struct(Info)#`. IDs that share a name are numbered, `#1`, `#2`, in the
    order a walk of the storage entries and the types they use first reaches
    them, so equal layouts agree whatever their IDs. An ID the walk does not
    reach keeps its plain name, and an error if another ID has it.
    """
    labels = {}
    for layout in layouts.values():
        types = layout.get("types") if isinstance(layout, dict) else None
        for key, entry in (types if isinstance(types, dict) else {}).items():
            match = STORAGE_TYPE_ID.match(key)
            label = entry.get("label") if isinstance(entry, dict) else None
            if match and isinstance(label, str):
                if not STORAGE_TYPE_LABEL.fullmatch(label):
                    raise ValueError(f"malformed type label {label!r} in {where}")
                name = f"{match[1]}{match[2]}{{{label}}}"
                if (other := labels.setdefault(match[0], name)) != name:
                    raise ValueError(f"{where}: {match[0]} is both {other} and {name}")

    # Each ID with its bare name, in first-use order; malformed parts are skipped
    # here and rejected by `storage_layout`.
    order = {}

    def visit(variables, types, seen):
        for variable in variables if isinstance(variables, list) else ():
            if isinstance(variable, dict):
                walk(variable.get("type"), types, seen)

    def walk(type_id, types, seen):
        if not isinstance(type_id, str) or type_id in seen:
            return
        seen.add(type_id)
        for match in STORAGE_TYPE_ID.finditer(type_id):
            order.setdefault(match[0], match[1] + match[2])
        entry = types.get(type_id) if isinstance(types, dict) else None
        if isinstance(entry, dict):
            for field in ("key", "value", "base"):
                walk(entry.get(field), types, seen)
            visit(entry.get("members"), types, seen)

    for layout in layouts.values():
        if isinstance(layout, dict):
            visit(layout.get("storage"), layout.get("types"), set())
    groups = defaultdict(list)
    for raw, bare in order.items():
        groups[labels.get(raw, bare)].append(raw)
    names = dict(labels)
    for name, raws in groups.items():
        if len(raws) > 1:
            names.update((raw, f"{name}#{n}") for n, raw in enumerate(raws, 1))
    result = {
        key: storage_layout(layout, f"{where} {key}", names)
        for key, layout in layouts.items()
    }
    seen = {}
    for raw, name in sorted(names.items()):
        if (other := seen.setdefault(name, raw)) != raw:
            raise ValueError(f"{where}: {other} and {raw} both become {name}")
    return result


def storage_layout(layout, where, names):
    """Rename user-defined types by `names`, adding bare names, and drop `astId`."""
    malformed = ValueError(f"malformed storage layout in {where}")
    if (
        not isinstance(layout, dict)
        or not isinstance(layout.get("storage"), list)
        or not isinstance(layout.get("types"), (dict, type(None)))
    ):
        raise malformed

    def strip(match):
        return names.setdefault(match[0], f"{match[1]}{match[2]}#")

    def rename_type(type_id):
        if not isinstance(type_id, str) or STORAGE_TYPE_WITHOUT_ID.search(type_id):
            raise malformed
        return STORAGE_TYPE_ID.sub(strip, type_id)

    def rename(key, value):
        if isinstance(value, list):
            # Only `storage` and `members` hold lists, of variables.
            if not all(
                isinstance(v, dict) and isinstance(v.get("type"), str) for v in value
            ):
                raise malformed
            return [rename(None, v) for v in value]
        if isinstance(value, dict) and key == "types":
            if not all(isinstance(v, dict) for v in value.values()):
                raise malformed
            # Distinct keys stay distinct: `storage_layouts` rejects two IDs with one name.
            return {rename_type(k): rename(k, v) for k, v in value.items()}
        if isinstance(value, dict):
            return {k: rename(k, v) for k, v in value.items() if k != "astId"}
        if key in STORAGE_TYPE_FIELDS:
            return rename_type(value)
        return value

    return rename(None, layout)


def artifact(output, comparator, policy):
    contracts = contract_outputs(output)
    result = {}
    missing = set()
    errors = {}
    # Solc before 0.8.27 emits no transient layouts, so an output without any
    # counts them as empty.
    transient = any(
        isinstance(contract, dict)
        and contract.get("transientStorageLayout") is not None
        for group in contracts.values()
        if isinstance(group, dict)
        for contract in group.values()
    )
    for path, group in contracts.items():
        if not isinstance(group, dict) or not group:
            raise ValueError("malformed contract outputs")
        result[path] = {}
        for name, contract in group.items():
            if not isinstance(contract, dict):
                raise ValueError("malformed contract artifact")
            if comparator == "storage-layout":
                layouts = result[path][name] = {}
                present = {}
                for key in STORAGE_LAYOUTS:
                    value = contract.get(key)
                    if (
                        value is None
                        and key == "transientStorageLayout"
                        and not transient
                    ):
                        value = {"storage": [], "types": None}
                    if value is None:
                        missing.add((path, name, key))
                        layouts[key] = {"available": False}
                    else:
                        present[key] = value
                # A bad contract fails the check but leaves other contracts compared.
                try:
                    for key, value in storage_layouts(
                        present, f"{path}:{name}"
                    ).items():
                        layouts[key] = {"available": True, "value": value}
                except ValueError as error:
                    errors[path, name] = str(error)
                continue
            if comparator == "methods":
                evm = contract.get("evm", {})
                if not isinstance(evm, dict):
                    raise ValueError("malformed EVM output")
                value = evm.get("methodIdentifiers")
            else:
                value = contract.get(comparator)
            if value is None:
                missing.add((path, name))
                result[path][name] = {"available": False}
            else:
                if comparator == "abi":
                    value = abi_entries(value, policy)
                elif not isinstance(value, dict):
                    raise ValueError(f"{comparator} must be an object")
                if comparator == "methods" and any(
                    not isinstance(v, str)
                    or len(v) != 8
                    or any(c not in "0123456789abcdefABCDEF" for c in v)
                    for v in value.values()
                ):
                    raise ValueError("invalid method selector")
                if comparator == "methods":
                    value = {key: selector.lower() for key, selector in value.items()}
                result[path][name] = {"available": True, "value": value}
    return result, missing, errors


def ast_children(value, path):
    """Yield (path, node) for the nodes at the top of a field value."""
    if isinstance(value, dict):
        if "nodeType" in value:
            yield path, value
        else:
            for key, child in value.items():
                yield from ast_children(child, pointer(path, key))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            yield from ast_children(child, f"{path}/{index}")


def ast_pairs(left, right, path):
    """Yield (path, left, right) for nodes at the same position in two values."""
    if isinstance(left, dict) and isinstance(right, dict):
        if "nodeType" in left and "nodeType" in right:
            yield path, left, right
        elif "nodeType" not in left and "nodeType" not in right:
            for key in sorted(left.keys() & right.keys()):
                yield from ast_pairs(left[key], right[key], pointer(path, key))
    elif isinstance(left, list) and isinstance(right, list):
        for index, pair in enumerate(zip(left, right)):
            yield from ast_pairs(*pair, f"{path}/{index}")


def ast_roots(output):
    """Map each source with an AST to a pseudo node holding it, by JSON pointer."""
    contract_outputs(output, require_contracts=False)
    sources = output.get("sources", {})
    if not isinstance(sources, dict) or not all(
        isinstance(s, dict) for s in sources.values()
    ):
        raise ValueError("malformed source outputs")
    roots = {}
    for name, source in sources.items():
        ast = source.get("ast")
        if ast is not None:
            if not isinstance(ast, dict) or not isinstance(ast.get("nodeType"), str):
                raise ValueError("malformed AST")
            roots[pointer("/sources", name)] = {"ast": ast}
    return roots


def ast_index(roots):
    """Name each node by its pointer, and count node types."""
    ids = {}
    types = Counter()
    stack = [pair for path, root in roots.items() for pair in ast_children(root, path)]
    while stack:
        path, node = stack.pop()
        types[node["nodeType"]] += 1
        if "id" in node:
            if type(node["id"]) is not int or node["id"] < 0:
                raise ValueError(f"invalid AST node ID {node['id']!r} at {path}")
            if node["id"] in ids:
                raise ValueError(f"duplicate AST node ID {node['id']} at {path}")
            ids[node["id"]] = path
        for key, value in node.items():
            if isinstance(value, (dict, list)):
                stack.extend(ast_children(value, pointer(path, key)))
    return ids, types


def ast_translate(key, value, ref, source):
    """Rename node IDs and span sources in a field value, and cut child nodes to their type."""
    if isinstance(value, dict):
        if "nodeType" in value:
            return {"nodeType": value["nodeType"]}
        if key == "internalFunctionIDs":
            return {str(ref(int(k))): v for k, v in value.items()}
        if key == "exportedSymbols":
            return {k: [ref(i) for i in v] for k, v in value.items()}
        return {k: ast_translate(k, v, ref, source) for k, v in value.items()}
    if isinstance(value, list):
        items = [ast_translate(key, v, ref, source) for v in value]
        return sorted(items, key=str) if key in AST_UNORDERED_FIELDS else items
    if type(value) is int and key in AST_ID_FIELDS:
        return ref(value)
    if isinstance(value, str) and key in AST_SPAN_FIELDS:
        return AST_SPAN.sub(lambda m: m[1] + source(int(m[2])), value)
    if isinstance(value, str) and key == "typeIdentifier":
        return AST_TYPE_ID.sub(lambda m: m[1] + str(ref(int(m[2]))), value)
    return value


def ast_source(output):
    """Rename a source index to the name of its source."""
    names = {
        s["id"]: name for name, s in output.get("sources", {}).items() if "id" in s
    }
    return lambda n: names.get(n, str(n))


def ast_ref(ids, side):
    """Rename an ID to its node; one with no node never agrees with the other side."""
    return lambda n: n if n < 0 else ids.get(n, f"?{n}@{side}")


def compare_ast(left, right, rules):
    """Compare ASTs node by node, counting agreement per node type and field.

    Each node ID becomes the JSON pointer of its node, so a reference agrees only
    when both sides point at the node in the same place. Negative builtin IDs stay.
    The source index of every span becomes the source's name.
    Fields that hold child nodes compare only the child's type; the children are
    compared in turn. Ignored and presence-only fields skip the nodes below them.
    """
    roots = [ast_roots(left), ast_roots(right)]
    indexes = [ast_index(side) for side in roots]
    sources = [ast_source(left), ast_source(right)]
    refs = [ast_ref(ids, side) for (ids, _), side in zip(indexes, ("left", "right"))]
    by_field = {}
    for number, rule in enumerate(rules):
        by_field.setdefault((rule["nodeType"], rule["field"]), number)
    rows = defaultdict(Counter)
    applied = Counter()
    found = []
    samples = Counter()
    omitted = 0
    groups = Counter()
    # Each source's `ast` is a field of a pseudo node, so missing sources count too.
    stack = [
        (path, "(sources)", roots[0].get(path, {}), roots[1].get(path, {}))
        for path in sorted(roots[0].keys() | roots[1].keys(), reverse=True)
    ]
    paired = Counter()
    while stack:
        path, node_type, l, r = stack.pop()
        paired[node_type] += 1
        children = []
        for key in [*l, *(k for k in r if k not in l)]:
            # Paired nodes share a type, and each ID is its node's own pointer.
            if key in ("id", "nodeType") and key in l and key in r:
                continue
            row = rows[node_type, key]
            number = by_field.get((node_type, key), by_field.get(("*", key)))
            action = rules[number]["action"] if number is not None else None
            new = []
            if action == "ignore":
                applied[number] += 1
                row["ignored"] += 1
            elif key not in l or key not in r:
                side = "left" if key not in l else "right"
                row["missing_" + side] += 1
                new = [
                    {
                        "path": pointer(path, key),
                        "kind": "missing-" + side,
                        "left": ast_translate(key, l[key], refs[0], sources[0])
                        if key in l
                        else None,
                        "right": ast_translate(key, r[key], refs[1], sources[1])
                        if key in r
                        else None,
                    }
                ]
            elif action == "presence":
                applied[number] += 1
                row["equal"] += 1
            else:
                lv = ast_translate(key, l[key], refs[0], sources[0])
                rv = ast_translate(key, r[key], refs[1], sources[1])
                if (
                    type(lv) is type(rv)
                    and lv == rv
                    and not isinstance(lv, (dict, list))
                ):
                    row["equal"] += 1
                elif new := differences(lv, rv, pointer(path, key)):
                    row["different"] += 1
                else:
                    row["equal"] += 1
                if isinstance(l[key], (dict, list)):
                    for p, x, y in ast_pairs(l[key], r[key], pointer(path, key)):
                        if x["nodeType"] == y["nodeType"]:
                            children.append((p, x["nodeType"], x, y))
            if new:
                groups[f"{node_type}.{key}"] += len(new)
            for difference in new:
                if samples[node_type, key] < AST_SAMPLES:
                    samples[node_type, key] += 1
                    found.append(difference)
                else:
                    omitted += 1
        # Pop in document order, so the samples are the first differences.
        stack.extend(reversed(children))
    counters = ("equal", "different", "missing_left", "missing_right")
    types = [types for _, types in indexes]
    return {
        "comparator": "ast",
        "version": VERSION,
        "status": "unsupported"
        if not any(roots)
        else "different"
        if found
        else "equal",
        "differences": found,
        "omitted": omitted,
        "groups": dict(groups),
        "fields": [
            {
                "nodeType": node_type,
                "field": field,
                "compared": sum(row[c] for c in counters),
                **{c: row[c] for c in (*counters, "ignored")},
            }
            for (node_type, field), row in sorted(rows.items())
        ],
        "node_types": {
            node_type: {
                "left": types[0][node_type],
                "right": types[1][node_type],
                "paired": paired[node_type],
            }
            for node_type in sorted(types[0].keys() | types[1].keys())
        },
        "rules": [{**rule, "applied": applied[n]} for n, rule in enumerate(rules)],
    }


class AstSummary:
    """Add up the fields, node counts and rule use of ast results."""

    def __init__(self):
        self.fields = defaultdict(Counter)
        self.types = defaultdict(Counter)
        self.applied = Counter()
        self.rules = 0
        self.results = 0

    def add(self, result):
        for row in result["fields"]:
            self.fields[row["nodeType"], row["field"]].update(
                {k: v for k, v in row.items() if type(v) is int}
            )
        for node_type, counts in result["node_types"].items():
            self.types[node_type].update(counts)
        for number, rule in enumerate(result["rules"]):
            self.applied[number] += rule["applied"]
        self.rules = len(result["rules"])
        self.results += 1

    def report(self):
        return {
            "fields": [
                {"nodeType": node_type, "field": field, **counts}
                for (node_type, field), counts in sorted(self.fields.items())
            ],
            "node_types": {
                name: dict(counts) for name, counts in sorted(self.types.items())
            },
            "never_emitted": sorted(
                n for n, c in self.types.items() if c["left"] and not c["right"]
            ),
            "unused_rules": [n for n in range(self.rules) if not self.applied[n]],
        }


def compare_outputs(left, right, comparator="abi", policy="interface", rules=()):
    if comparator not in COMPARATORS or policy not in {"exact", "interface"}:
        raise ValueError("unknown comparator or ABI policy")
    version = COMPARATORS[comparator]
    try:
        if comparator == "ast":
            return {**compare_ast(left, right, rules), "policy": policy}
        a, missing_a, errors_a = artifact(left, comparator, policy)
        b, missing_b, errors_b = artifact(right, comparator, policy)
        # Only the error reports a contract that failed on either side.
        for path, name in errors_a.keys() | errors_b.keys():
            for side in (a, b):
                if (group := side.get(path)) is not None:
                    group.pop(name, None)
                    if not group:
                        del side[path]
        diff = differences(a, b, "/contracts")
        errors = [f"left: {e}" for e in errors_a.values()]
        errors += [f"right: {e}" for e in errors_b.values()]
        status = (
            "error"
            if errors
            else "unsupported"
            if missing_a & missing_b
            else "different"
            if diff
            else "equal"
        )
        return {
            "comparator": comparator,
            "version": version,
            "policy": policy,
            "status": status,
            "differences": diff,
            **({"error": "\n".join(errors)} if errors else {}),
        }
    # Every comparator walks the outputs recursively, so absurdly deep JSON can
    # overflow the stack; report that as an error instead of aborting the run.
    except (ValueError, KeyError, TypeError, RecursionError) as error:
        return {
            "comparator": comparator,
            "version": version,
            "policy": policy,
            "status": "error",
            "differences": [],
            "error": str(error),
        }


def rule_index(rules):
    index = {}
    for number, rule in enumerate(rules):
        key = (
            rule["comparator"],
            rule["version"],
            rule["policy"],
            json.dumps(rule["difference"], sort_keys=True),
        )
        index.setdefault(key, (number, rule["reason"]))
    return index


def expectations(report, rules, index=None):
    """Match exact differences, scoped to comparator, policy and version."""
    if index is None:
        index = rule_index(rules)
    matched = set()
    for result in report["results"]:
        for difference in result["differences"]:
            key = (
                result["comparator"],
                result["version"],
                result["policy"],
                json.dumps(difference, sort_keys=True),
            )
            if key in index:
                number, reason = index[key]
                difference["expected"] = reason
                matched.add(number)
    report["unused_expectations"] = [i for i in range(len(rules)) if i not in matched]
    report["failed"] = any(
        result["status"] in {"error", "unsupported"}
        or result.get("omitted")
        or any("expected" not in d for d in result["differences"])
        for result in report["results"]
    )
    return matched


class Tests(unittest.TestCase):
    def output(self):
        return {
            "contracts": {
                "C.sol": {
                    "C": {
                        "abi": [
                            {
                                "type": "function",
                                "name": "f",
                                "inputs": [
                                    {
                                        "name": "x",
                                        "type": "tuple[]",
                                        "internalType": "struct C.S[]",
                                        "components": [
                                            {"name": "a", "type": "uint256"},
                                            {"name": "b", "type": "bool"},
                                        ],
                                    }
                                ],
                                "outputs": [{"name": "", "type": "uint256"}],
                                "stateMutability": "view",
                            },
                            {
                                "type": "event",
                                "name": "E",
                                "inputs": [
                                    {"name": "x", "type": "uint256", "indexed": True}
                                ],
                                "anonymous": False,
                            },
                            {"type": "error", "name": "Oops", "inputs": []},
                            {
                                "type": "constructor",
                                "inputs": [],
                                "stateMutability": "nonpayable",
                            },
                            {"type": "fallback", "stateMutability": "payable"},
                            {"type": "receive", "stateMutability": "payable"},
                        ],
                        "evm": {
                            "methodIdentifiers": {"f((uint256,bool)[])": "12345678"}
                        },
                        "userdoc": {"version": 1, "methods": {}},
                        "devdoc": {"version": 1, "methods": {}},
                    }
                }
            }
        }

    def test_order_and_names(self):
        left = self.output()
        right = copy.deepcopy(left)
        right["contracts"]["C.sol"]["C"]["abi"].reverse()
        self.assertEqual(
            compare_outputs(left, right, policy="exact")["status"], "equal"
        )
        param = right["contracts"]["C.sol"]["C"]["abi"][-1]["inputs"][0]
        param["name"] = "renamed"
        param["internalType"] = "struct Other.S[]"
        param["components"][0]["name"] = "renamed"
        self.assertEqual(compare_outputs(left, right)["status"], "equal")
        self.assertEqual(
            compare_outputs(left, right, policy="exact")["status"], "different"
        )

    def test_semantic_changes(self):
        for index, field, value in [
            (0, "stateMutability", "pure"),
            (0, "outputs", []),
            (1, "anonymous", True),
            (2, "name", "Other"),
            (3, "stateMutability", "payable"),
            (4, "stateMutability", "nonpayable"),
            (5, "stateMutability", "nonpayable"),
        ]:
            with self.subTest(index=index, field=field):
                left = self.output()
                right = copy.deepcopy(left)
                right["contracts"]["C.sol"]["C"]["abi"][index][field] = value
                self.assertEqual(compare_outputs(left, right)["status"], "different")
        left = self.output()
        for change in (
            "tuple-order",
            "indexed",
            "duplicate",
            "missing-contract",
            "extra-contract",
        ):
            with self.subTest(change=change):
                right = copy.deepcopy(left)
                abi = right["contracts"]["C.sol"]["C"]["abi"]
                if change == "tuple-order":
                    abi[0]["inputs"][0]["components"].reverse()
                elif change == "indexed":
                    abi[1]["inputs"][0]["indexed"] = False
                elif change == "duplicate":
                    abi.append(copy.deepcopy(abi[0]))
                elif change == "missing-contract":
                    right["contracts"]["C.sol"] = {
                        "Other": right["contracts"]["C.sol"].pop("C")
                    }
                else:
                    right["contracts"]["C.sol"]["Extra"] = copy.deepcopy(
                        right["contracts"]["C.sol"]["C"]
                    )
                self.assertEqual(compare_outputs(left, right)["status"], "different")

    def test_json_and_missing_outputs(self):
        left = self.output()
        for comparator in ("abi", "methods", "userdoc", "devdoc"):
            with self.subTest(comparator=comparator):
                self.assertEqual(
                    compare_outputs(left, left, comparator)["status"], "equal"
                )
        self.assertEqual(compare_outputs(left, left, "ast")["status"], "unsupported")
        right = copy.deepcopy(left)
        right["contracts"]["C.sol"]["C"]["evm"]["methodIdentifiers"] = {
            "f((uint256,bool)[])": "abcdef01"
        }
        self.assertEqual(compare_outputs(left, right, "methods")["status"], "different")
        right["contracts"]["C.sol"]["C"].pop("abi")
        self.assertEqual(compare_outputs(left, right)["status"], "different")
        self.assertEqual(compare_outputs(right, right)["status"], "unsupported")
        self.assertEqual(compare_outputs({}, {})["status"], "error")
        right = {"errors": [{"severity": "error"}]}
        self.assertEqual(compare_outputs(right, right)["status"], "error")
        self.assertEqual(
            differences({"x": None}, {}),
            [{"path": "/x", "kind": "missing-right", "left": None, "right": None}],
        )
        self.assertEqual(differences({"x": 1}, {"x": True})[0]["kind"], "value")

    def test_partial_unsupported_is_not_masked(self):
        left = self.output()
        right = copy.deepcopy(left)
        for output in (left, right):
            output["contracts"]["C.sol"]["Missing"] = {}
        right["contracts"]["C.sol"]["C"]["abi"][0]["stateMutability"] = "pure"
        result = compare_outputs(left, right)
        self.assertEqual(result["status"], "unsupported")
        self.assertEqual(len(result["differences"]), 1)
        report = {"results": [result]}
        rule = {key: result[key] for key in ("comparator", "version", "policy")}
        rule.update(difference=copy.deepcopy(result["differences"][0]), reason="known")
        expectations(report, [rule])
        self.assertTrue(report["failed"])

    def test_expected_does_not_mask_unexpected(self):
        left = self.output()
        right = copy.deepcopy(left)
        right["contracts"]["C.sol"]["C"]["abi"][0]["stateMutability"] = "pure"
        result = compare_outputs(left, right)
        rule = {key: result[key] for key in ("comparator", "version", "policy")}
        rule.update(
            difference=copy.deepcopy(result["differences"][0]),
            reason="tracked discrepancy",
        )
        report = {"results": [result]}
        expectations(report, [rule])
        self.assertFalse(report["failed"])
        right["contracts"]["C.sol"]["C"]["abi"][1]["anonymous"] = True
        report = {"results": [compare_outputs(left, right)]}
        expectations(report, [rule])
        self.assertTrue(report["failed"])
        report = {"results": [compare_outputs(left, left)]}
        expectations(report, [rule])
        self.assertEqual(report["unused_expectations"], [0])

    def storage_output(self, base=0, key="t_string_memory_ptr"):
        struct = f"t_struct(S){base + 1}_storage"
        mapping = f"t_mapping({key},{struct})"
        array = f"t_array(t_contract(D){base + 2})dyn_storage"
        udvt = f"t_userDefinedValueType(U){base + 3}"

        def entry(label, slot, ty):
            return {
                "astId": base + 10 + int(slot),
                "contract": "C.sol:C",
                "label": label,
                "offset": 0,
                "slot": slot,
                "type": ty,
            }

        types = {
            mapping: {
                "encoding": "mapping",
                "key": key,
                "label": "mapping(string => struct C.S)",
                "numberOfBytes": "32",
                "value": struct,
            },
            struct: {
                "encoding": "inplace",
                "label": "struct C.S",
                "members": [
                    entry("u", "0", udvt),
                    entry("e", "1", f"t_enum(E){base + 4}"),
                ],
                "numberOfBytes": "64",
            },
            array: {
                "base": f"t_contract(D){base + 2}",
                "encoding": "dynamic_array",
                "label": "contract D[]",
                "numberOfBytes": "32",
            },
        }
        layout = {
            "storage": [entry("m", "0", mapping), entry("a", "1", array)],
            # Key order carries no meaning.
            "types": dict(reversed(types.items())) if base else types,
        }
        transient = {"storage": [entry("t", "0", udvt)], "types": {udvt: {}}}
        contract = {"storageLayout": layout, "transientStorageLayout": transient}
        return {"contracts": {"C.sol": {"C": contract}}}

    def test_storage_layout_ast_ids(self):
        left = self.storage_output()
        result = compare_outputs(left, self.storage_output(100), "storage-layout")
        self.assertEqual(result["status"], "equal")
        self.assertEqual(result["version"], STORAGE_LAYOUT_VERSION)
        right = self.storage_output(100)
        right["contracts"]["C.sol"]["C"]["storageLayout"]["storage"].reverse()
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(result["status"], "different")
        right = self.storage_output(100, key="t_string")
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(result["status"], "different")
        path = "/contracts/C.sol/C/storageLayout/value"
        self.assertIn(
            {
                "path": path + "/storage/0/type",
                "kind": "value",
                "left": "t_mapping(t_string_memory_ptr,t_struct(S){struct C.S}_storage)",
                "right": "t_mapping(t_string,t_struct(S){struct C.S}_storage)",
            },
            result["differences"],
        )
        rule = {key: result[key] for key in ("comparator", "version", "policy")}
        rules = [
            {**rule, "difference": d, "reason": "x"} for d in result["differences"]
        ]
        report = {"results": [result]}
        expectations(report, rules)
        self.assertFalse(report["failed"])

    def test_storage_layout_same_names(self):
        def output(tick, position, swap=False):
            def struct(scope, member):
                return {
                    "encoding": "inplace",
                    "label": f"struct {scope}.Info",
                    "members": [{"label": member, "slot": "0", "type": "t_uint256"}],
                    "numberOfBytes": "32",
                }

            ticks, positions = (position, tick) if swap else (tick, position)
            storage = [
                {
                    "label": "ticks",
                    "slot": "0",
                    "type": f"t_struct(Info){ticks}_storage",
                },
                {
                    "label": "f",
                    "slot": "1",
                    "type": f"t_function_internal_nonpayable(t_struct(Info){tick}_memory_ptr)returns()",
                },
                {
                    "label": "positions",
                    "slot": "2",
                    "type": f"t_struct(Info){positions}_storage",
                },
            ]
            types = {
                f"t_struct(Info){tick}_storage": struct("Tick", "a"),
                f"t_struct(Info){position}_storage": struct("Position", "b"),
            }
            layout = {"storage": storage, "types": types}
            contract = {"storageLayout": layout, "transientStorageLayout": None}
            return {"contracts": {"P.sol": {"P": contract}}}

        result = compare_outputs(output(3, 11), output(40, 12), "storage-layout")
        self.assertEqual(result["status"], "equal")
        self.assertEqual(result["differences"], [])
        renamed = json.dumps(output(40, 12)).replace("(Info)40", "(Other)40")
        result = compare_outputs(output(3, 11), json.loads(renamed), "storage-layout")
        self.assertNotEqual(result["differences"], [])
        result = compare_outputs(output(3, 11), output(40, 12, True), "storage-layout")
        self.assertIn(
            {
                "path": "/contracts/P.sol/P/storageLayout/value/storage/0/type",
                "kind": "value",
                "left": "t_struct(Info){struct Tick.Info}_storage",
                "right": "t_struct(Info){struct Position.Info}_storage",
            },
            result["differences"],
        )

    def test_storage_layout_name_collision(self):
        left = self.storage_output()
        layout = left["contracts"]["C.sol"]["C"]["storageLayout"]
        # A `struct S` that no variable uses has no place in the walk.
        layout["types"]["t_struct(S)7_storage"] = layout["types"][
            "t_struct(S)1_storage"
        ]
        right = self.storage_output()
        for output in (left, right):
            output["contracts"]["D.sol"] = copy.deepcopy(right["contracts"]["C.sol"])
        right["contracts"]["D.sol"]["C"]["storageLayout"]["storage"].pop()
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(result["status"], "error")
        self.assertIn(
            "left: C.sol:C: t_struct(S)1 and t_struct(S)7 both become"
            " t_struct(S){struct C.S}",
            result["error"],
        )
        self.assertEqual(
            [d["path"] for d in result["differences"]],
            ["/contracts/D.sol/C/storageLayout/value/storage/1"],
        )
        del right["contracts"]["C.sol"]
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(
            [d["path"] for d in result["differences"]],
            ["/contracts/D.sol/C/storageLayout/value/storage/1"],
        )

    def test_storage_layout_shared_names(self):
        # Solc 0.8.38 layouts of structs used only in function types, and of two
        # file-level `struct S` from different files.
        def output(ids=(3, 7, 39, 43), swap=False):
            tick, position = (
                f"t_function_internal_nonpayable(t_struct(Info){i}_memory_ptr)returns()"
                for i in ids[:2]
            )
            a, b = (f"t_struct(S){i}_storage" for i in ids[2:])

            def struct(member):
                members = [{"label": member, "slot": "0", "type": "t_uint256"}]
                return {"label": "struct S", "members": members}

            def contract(order, types):
                order = order[::-1] if swap else order
                storage = [
                    {"label": label, "slot": "0", "type": ty}
                    for label, ty in zip("xy", order)
                ]
                layout = {"storage": storage, "types": types}
                transient = {"storage": [], "types": None}
                return {
                    "C": {"storageLayout": layout, "transientStorageLayout": transient}
                }

            return {
                "contracts": {
                    "fn.sol": contract(
                        [tick, position],
                        {
                            tick: {"label": "function (struct Tick.Info)"},
                            position: {"label": "function (struct Position.Info)"},
                        },
                    ),
                    "two.sol": contract([a, b], {a: struct("a"), b: struct("b")}),
                }
            }

        result = compare_outputs(output(), output(), "storage-layout")
        self.assertEqual(result["status"], "equal")
        result = compare_outputs(output(), output((70, 30, 90, 50)), "storage-layout")
        self.assertEqual(result["status"], "equal")
        result = compare_outputs(output(), output(swap=True), "storage-layout")
        self.assertEqual(
            [(d["path"].split("/value/")[1], d["left"]) for d in result["differences"]],
            [
                (
                    "types/t_function_internal_nonpayable(t_struct(Info)#1_memory_ptr)returns()/label",
                    "function (struct Tick.Info)",
                ),
                (
                    "types/t_function_internal_nonpayable(t_struct(Info)#2_memory_ptr)returns()/label",
                    "function (struct Position.Info)",
                ),
                ("types/t_struct(S){struct S}#1_storage/members/0/label", "a"),
                ("types/t_struct(S){struct S}#2_storage/members/0/label", "b"),
            ],
        )

    def test_storage_layout_malformed_or_ambiguous(self):
        def hash_key(c):
            types = c["storageLayout"]["types"]
            types["t_struct(S){struct C.S}_storage"] = types["t_struct(S)1_storage"]

        def forged_label(c):
            types = c["storageLayout"]["types"]
            types["t_struct(S)1_storage"]["label"] = "struct C.S}_storage,t_struct(T){T"

        def two_labels(c):
            types = c["storageLayout"]["types"]
            types["t_struct(S)1_storage_ptr"] = {**types["t_struct(S)1_storage"]}
            types["t_struct(S)1_storage_ptr"]["label"] = "struct D.S"

        def normalized_row(c):
            c["storageLayout"]["storage"][0]["type"] = "t_struct(S){struct C.S}_storage"

        for edit, message in [
            (hash_key, "malformed"),
            (normalized_row, "malformed"),
            (forged_label, "malformed type label"),
            (two_labels, "C.sol:C: t_struct(S)1 is both"),
            (lambda c: c["storageLayout"]["storage"].append(None), "malformed"),
            (lambda c: c["storageLayout"]["types"].update(x=None), "malformed"),
            (lambda c: c["storageLayout"]["storage"][0].update(type=1), "malformed"),
        ]:
            with self.subTest(message):
                output = self.storage_output()
                edit(output["contracts"]["C.sol"]["C"])
                result = compare_outputs(
                    self.storage_output(), output, "storage-layout"
                )
                self.assertEqual(result["status"], "error")
                self.assertIn("right: " + message, result["error"])

    def test_deep_output_is_an_error(self):
        output = self.storage_output()
        deep = {}
        for _ in range(10_000):
            deep = {"x": deep}
        output["contracts"]["C.sol"]["C"]["storageLayout"]["types"]["x"] = deep
        result = compare_outputs(self.storage_output(), output, "storage-layout")
        self.assertEqual(result["status"], "error")

    def test_storage_layout_missing(self):
        left = self.storage_output()
        left["contracts"]["D.sol"] = copy.deepcopy(left["contracts"]["C.sol"])
        right = copy.deepcopy(left)
        del right["contracts"]["C.sol"]["C"]["transientStorageLayout"]
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(result["status"], "different")
        self.assertEqual(
            [d["path"] for d in result["differences"]],
            [
                "/contracts/C.sol/C/transientStorageLayout/available",
                "/contracts/C.sol/C/transientStorageLayout/value",
            ],
        )
        result = compare_outputs(right, right, "storage-layout")
        self.assertEqual(result["status"], "unsupported")
        empty = {"storage": [], "types": None}
        left["contracts"]["C.sol"]["C"]["transientStorageLayout"] = empty
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(result["status"], "different")
        # Solc before 0.8.27 emits no transient layouts at all.
        del right["contracts"]["D.sol"]["C"]["transientStorageLayout"]
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(
            {d["path"].split("/value/")[0] for d in result["differences"]},
            {"/contracts/D.sol/C/transientStorageLayout"},
        )
        left["contracts"]["D.sol"]["C"]["transientStorageLayout"] = empty
        result = compare_outputs(left, right, "storage-layout")
        self.assertEqual(result["status"], "equal")

    def ast_output(self, renumber=False):
        names = [
            "S",
            "v",
            "f",
            "body",
            "stmt",
            "ident",
            "member",
            "msg",
            "C",
            "D",
            "unit",
        ]
        ids = {k: 1000 - n if renumber else n for n, k in enumerate(names, 1)}
        index = 7 if renumber else 0
        types = {
            "typeIdentifier": f"t_struct$_S_${ids['S']}_storage_ptr",
            "typeString": "struct C.S storage pointer",
        }
        msg = {
            "id": ids["msg"],
            "nodeType": "Identifier",
            "name": "msg",
            "overloadedDeclarations": [],
            "referencedDeclaration": -15,
            "typeDescriptions": {
                "typeIdentifier": f"t_type$_t_super$_C_${ids['C']}_$",
                "typeString": "t_module_999",
            },
        }
        ident = {
            "id": ids["ident"],
            "nodeType": "Identifier",
            "name": "v",
            "overloadedDeclarations": [],
            "referencedDeclaration": ids["v"],
            "typeDescriptions": types,
            "argumentTypes": [types],
        }
        member = {
            "id": ids["member"],
            "nodeType": "MemberAccess",
            "memberName": "sender",
            "expression": msg,
        }
        statements = [
            {"id": ids["stmt"], "nodeType": "ExpressionStatement", "expression": ident},
            {
                "id": ids["stmt"] + 100,
                "nodeType": "ExpressionStatement",
                "expression": member,
            },
        ]
        nodes = [
            {
                "id": ids["S"],
                "nodeType": "StructDefinition",
                "name": "S",
                "src": f"5:3:{index}",
                "nameLocation": f"12:1:{index}",
            },
            {
                "id": ids["v"],
                "nodeType": "VariableDeclaration",
                "scope": ids["C"],
                "typeDescriptions": types,
            },
            {
                "id": ids["f"],
                "nodeType": "FunctionDefinition",
                "baseFunctions": [ids["S"], ids["v"]],
                "body": {
                    "id": ids["body"],
                    "nodeType": "Block",
                    "statements": statements,
                },
            },
        ]

        def contract(name, bases, nodes):
            return {
                "id": ids[name],
                "nodeType": "ContractDefinition",
                "name": name,
                "scope": ids["unit"],
                "linearizedBaseContracts": [ids[b] for b in bases],
                "contractDependencies": [ids["C"], ids["D"]],
                "nodes": nodes,
            }

        unit = {
            "id": ids["unit"],
            "nodeType": "SourceUnit",
            "exportedSymbols": {"C": [ids["C"]], "D": [ids["D"]]},
            "nodes": [contract("C", "CD", nodes), contract("D", "D", [])],
        }
        return {"sources": {"A.sol": {"id": index, "ast": unit}}}

    def summarize(self, *results):
        total = AstSummary()
        for result in results:
            total.add(result)
        return total.report()

    def ast_rows(self, result):
        return {(r["nodeType"], r["field"]): r for r in result["fields"]}

    def test_ast_renumbering(self):
        left = self.ast_output()
        right = self.ast_output(renumber=True)
        result = compare_outputs(left, right, "ast")
        self.assertEqual(result["status"], "equal")
        self.assertEqual(result["differences"], [])
        self.assertTrue(all(r["equal"] == r["compared"] for r in result["fields"]))
        self.assertEqual(
            {r["field"] for r in result["fields"]} & {"id", "nodeType"}, set()
        )
        dangling = self.ast_output()
        dangling["sources"]["A.sol"]["ast"]["nodes"][0]["scope"] = 7
        other = self.ast_output(renumber=True)
        other["sources"]["A.sol"]["ast"]["nodes"][0]["scope"] = 7
        self.assertEqual(compare_outputs(dangling, other, "ast")["status"], "different")
        right["sources"]["A.sol"]["ast"]["nodes"][0]["contractDependencies"].reverse()
        self.assertEqual(compare_outputs(left, right, "ast")["status"], "equal")

    def test_ast_wrong_references_differ(self):
        left = self.ast_output()

        def renamed(edit):
            right = self.ast_output(renumber=True)
            unit = right["sources"]["A.sol"]["ast"]
            body = unit["nodes"][0]["nodes"][2]["body"]
            statements = body["statements"]
            edit(unit, statements[0]["expression"], statements[1]["expression"])
            return compare_outputs(left, right, "ast")

        for name, edit, field in [
            (
                "reference",
                lambda unit, ident, member: ident.update(referencedDeclaration=999),
                ("Identifier", "referencedDeclaration"),
            ),
            (
                "builtin",
                lambda unit, ident, member: member["expression"].update(
                    referencedDeclaration=-16
                ),
                ("Identifier", "referencedDeclaration"),
            ),
            (
                "unresolved",
                lambda unit, ident, member: ident.update(referencedDeclaration=7),
                ("Identifier", "referencedDeclaration"),
            ),
            (
                "type",
                lambda unit, ident, member: ident["typeDescriptions"].update(
                    typeIdentifier="t_struct$_S_$7_storage_ptr"
                ),
                ("Identifier", "typeDescriptions"),
            ),
            (
                "span",
                lambda unit, ident, member: unit["nodes"][0]["nodes"][0].update(
                    src="6:3:7"
                ),
                ("StructDefinition", "src"),
            ),
            (
                "order",
                lambda unit, ident, member: unit["nodes"][0][
                    "linearizedBaseContracts"
                ].reverse(),
                ("ContractDefinition", "linearizedBaseContracts"),
            ),
            (
                "exported",
                lambda unit, ident, member: unit["exportedSymbols"].update(
                    C=[unit["nodes"][1]["id"]]
                ),
                ("SourceUnit", "exportedSymbols"),
            ),
        ]:
            with self.subTest(name):
                result = renamed(edit)
                self.assertEqual(result["status"], "different")
                self.assertEqual(self.ast_rows(result)[field]["different"], 1)

    def test_ast_agreement_counts(self):
        left = self.ast_output()
        right = self.ast_output()
        unit = right["sources"]["A.sol"]["ast"]
        struct, variable, function = unit["nodes"][0]["nodes"]
        function["body"]["statements"].pop()
        del variable["typeDescriptions"]
        struct["extra"] = 1
        unit["nodes"][1]["name"] = "E"
        result = compare_outputs(left, right, "ast")
        rows = self.ast_rows(result)
        counts = lambda key: [
            rows[key][c]
            for c in ("compared", "equal", "different", "missing_left", "missing_right")
        ]
        self.assertEqual(counts(("Block", "statements")), [1, 0, 1, 0, 0])
        self.assertEqual(
            counts(("VariableDeclaration", "typeDescriptions")), [1, 0, 0, 0, 1]
        )
        self.assertEqual(counts(("StructDefinition", "extra")), [1, 0, 0, 1, 0])
        self.assertEqual(counts(("ContractDefinition", "name")), [2, 1, 1, 0, 0])
        self.assertNotIn(("MemberAccess", "memberName"), rows)
        self.assertEqual(
            result["node_types"]["MemberAccess"],
            {"left": 1, "right": 0, "paired": 0},
        )
        self.assertEqual(
            result["node_types"]["ExpressionStatement"],
            {"left": 2, "right": 1, "paired": 1},
        )
        self.assertEqual(
            sorted(d["path"] for d in result["differences"]),
            [
                "/sources/A.sol/ast/nodes/0/nodes/0/extra",
                "/sources/A.sol/ast/nodes/0/nodes/1/typeDescriptions",
                "/sources/A.sol/ast/nodes/0/nodes/2/body/statements/1",
                "/sources/A.sol/ast/nodes/1/name",
            ],
        )
        summary = self.summarize(result, result)
        self.assertEqual(summary["never_emitted"], ["MemberAccess"])
        self.assertEqual(
            summary["node_types"]["MemberAccess"],
            {"left": 2, "right": 0, "paired": 0},
        )

    def test_ast_rules_are_counted(self):
        left = self.ast_output()
        right = self.ast_output()
        unit = right["sources"]["A.sol"]["ast"]
        unit["nodes"][0]["name"] = "Other"
        unit["nodes"][0]["nodes"][0]["name"] = "T"
        rules = [
            {"nodeType": "*", "field": "name", "action": "ignore", "reason": "x"},
            {
                "nodeType": "SourceUnit",
                "field": "license",
                "action": "ignore",
                "reason": "x",
            },
        ]
        result = compare_outputs(left, right, "ast", rules=rules)
        self.assertEqual(result["status"], "equal")
        self.assertEqual([r["applied"] for r in result["rules"]], [5, 0])
        self.assertEqual(
            self.ast_rows(result)["ContractDefinition", "name"]["ignored"], 2
        )
        self.assertEqual(self.summarize(result)["unused_rules"], [1])
        rules[0]["action"] = "presence"
        self.assertEqual(
            compare_outputs(left, right, "ast", rules=rules)["status"], "equal"
        )
        del unit["nodes"][0]["name"]
        self.assertEqual(
            compare_outputs(left, right, "ast", rules=rules)["status"], "different"
        )

    def test_ast_errors_sources_and_samples(self):
        left = self.ast_output()
        right = self.ast_output()
        right["sources"]["A.sol"]["ast"]["nodes"][1]["id"] = 1
        self.assertEqual(compare_outputs(left, right, "ast")["status"], "error")
        for bad in ("7", -3, True):
            right = self.ast_output()
            right["sources"]["A.sol"]["ast"]["nodes"][1]["id"] = bad
            self.assertEqual(compare_outputs(left, right, "ast")["status"], "error")
        right = self.ast_output()
        right["sources"]["B.sol"] = right["sources"].pop("A.sol")
        result = compare_outputs(left, right, "ast")
        self.assertEqual(
            sorted(d["kind"] for d in result["differences"]),
            ["missing-left", "missing-right"],
        )
        right = self.ast_output()
        count = AST_SAMPLES + 5
        for index, contract in enumerate(right["sources"]["A.sol"]["ast"]["nodes"]):
            contract["nodes"] = [
                {"id": 100 * (index + 1) + n, "nodeType": "EnumDefinition", "name": "X"}
                for n in range(count)
            ]
        left = copy.deepcopy(right)
        for contract in left["sources"]["A.sol"]["ast"]["nodes"]:
            for item in contract["nodes"]:
                item["name"] = "Y"
        result = compare_outputs(left, right, "ast")
        self.assertEqual(len(result["differences"]), AST_SAMPLES)
        self.assertEqual(
            [d["path"] for d in result["differences"] if "/ast/nodes/0/" in d["path"]],
            [f"/sources/A.sol/ast/nodes/0/nodes/{n}/name" for n in range(AST_SAMPLES)],
        )
        self.assertEqual(result["omitted"], 2 * count - AST_SAMPLES)
        self.assertEqual(result["groups"], {"EnumDefinition.name": 2 * count})
        rule = {key: result[key] for key in ("comparator", "version", "policy")}
        rules = [
            {**rule, "difference": d, "reason": "x"} for d in result["differences"]
        ]
        report = {"results": [result]}
        expectations(report, rules)
        self.assertTrue(report["failed"])
