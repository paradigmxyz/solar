"""Versioned artifact comparisons; never reorder positional ABI components."""

import copy
import json
import unittest

from .artifacts import contract_outputs

VERSION = 1
COMPARATORS = ("abi", "methods", "userdoc", "devdoc")


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


def artifact(output, comparator, policy):
    contracts = contract_outputs(output)
    result = {}
    missing = set()
    for path, group in contracts.items():
        if not isinstance(group, dict) or not group:
            raise ValueError("malformed contract outputs")
        result[path] = {}
        for name, contract in group.items():
            if not isinstance(contract, dict):
                raise ValueError("malformed contract artifact")
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
    return result, missing


def compare_outputs(left, right, comparator="abi", policy="interface"):
    if comparator not in COMPARATORS or policy not in {"exact", "interface"}:
        raise ValueError("unknown comparator or ABI policy")
    try:
        a, missing_a = artifact(left, comparator, policy)
        b, missing_b = artifact(right, comparator, policy)
        diff = differences(a, b, "/contracts")
        status = (
            "unsupported" if missing_a & missing_b else "different" if diff else "equal"
        )
        return {
            "comparator": comparator,
            "version": VERSION,
            "policy": policy,
            "status": status,
            "differences": diff,
        }
    except (ValueError, KeyError, TypeError) as error:
        return {
            "comparator": comparator,
            "version": VERSION,
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
        for comparator in COMPARATORS:
            with self.subTest(comparator=comparator):
                self.assertEqual(
                    compare_outputs(left, left, comparator)["status"], "equal"
                )
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
