"""Structured MIR-to-Lean import: no lowering, codegen or regex instruction parser.

The compiler parser exports the schema-declared operations and typed SSA/CFG.
Only this constructor renderer is trusted, not a backend's executable image.
Independent replay reparses the original MIR and executes the Rust interpreter.
"""

import json
import subprocess

from . import native, world
from . import operations as ops
from .common import Program, Unsupported, literal


def number(value, limit=2**32):
    if type(value) is not int or not 0 <= value < limit:
        raise Unsupported("invalid MIR model integer")
    return str(value)


def width(value):
    result = number(value, 257)
    if result == "0":
        raise Unsupported("zero-width MIR value")
    return result


def value_type(value):
    kind = value.get("kind", "word")
    if kind == "word":
        return f".word {width(value['bits'])}"
    if kind in {"pointer", "slice"}:
        location = {
            "memory": 0,
            "calldata": 1,
            "returndata": 2,
            "storage": 3,
            "transient": 4,
        }.get(value["location"])
        if location is None:
            raise Unsupported("unknown MIR address space")
        return f".{kind} {location}"
    if kind == "product":
        return ".product [" + ",".join(value_type(v) for v in value["fields"]) + "]"
    raise Unsupported("unknown MIR value type")


def leaf_paths(value, path=()):
    if value.get("kind") == "product":
        return [
            leaf
            for i, field in enumerate(value["fields"])
            for leaf in leaf_paths(field, (*path, i))
        ]
    if value.get("kind") == "slice":
        return [
            (
                (*path, 0),
                {"kind": "pointer", "location": value["location"], "bits": 256},
            ),
            ((*path, 1), {"bits": 256}),
        ]
    return [(path, value)]


def leaf_types(value):
    return [ty for _, ty in leaf_paths(value)]


def argument_value(value, names):
    if value.get("kind") == "product":
        return (
            ".aggregate ["
            + ",".join(argument_value(v, names) for v in value["fields"])
            + "]"
        )
    if value.get("kind") == "slice":
        return (
            ".aggregate ["
            + ",".join(argument_value({"bits": 256}, names) for _ in range(2))
            + "]"
        )
    bits = width(value["bits"])
    return f".scalar ⟨{bits},{next(names)}⟩"


def operand(value):
    ty = value_type(value["type"])
    kind = value["kind"]
    if kind == "constant":
        bits = width(value["type"]["bits"])
        atom = f".constant ⟨{bits},{literal(value['value'], int(bits))}⟩"
    elif kind in {"arg", "local"}:
        atom = f".{'argument' if kind == 'arg' else 'local'} {number(value['index'])}"
    elif kind == "undefined":
        atom = ".undefined"
    else:
        raise Unsupported("unknown MIR model operand")
    return f"⟨({atom}),({ty})⟩"


def operands(values):
    return "[" + ",".join(map(operand, values)) + "]"


def operation(value, definitions):
    kind = value["kind"]
    args = value["args"]
    if kind == "opcode":
        name = value["name"]
        if name not in definitions:
            raise Unsupported("unknown schema opcode in MIR model")
        opcode = number(definitions[name]["opcode"], 256)
        bits = width(args[0]["type"]["bits"]) if args else "256"
        if name in ops.COMPARISONS:
            expression = f".comparison .{name} {bits}"
        elif name in ops.SHIFTS:
            expression = f".shift .{name} {bits}"
        elif name in ops.WORD_BINARY:
            expression = f".wordBinary .{name}"
        elif name in ops.BINARY:
            expression = f".binary .{name} {bits}"
        elif name in {"not", "clz"}:
            expression = f".unary .{name} {bits}"
        elif name == "iszero":
            expression = ".comparison .eq 256"
            args = [*args, {"kind": "constant", "value": "0", "type": {"bits": 256}}]
        elif name in {"addmod", "mulmod"}:
            expression = f".ternary .{name}"
        elif name in {
            "mload",
            "mstore",
            "mstore8",
            "mcopy",
            "msize",
            "keccak256",
            "calldataload",
            "calldatasize",
            "calldatacopy",
            "returndatasize",
            "returndatacopy",
        }:
            tag = {
                "mload": "load",
                "mstore": "store",
                "mstore8": "storeByte",
                "mcopy": "copy",
                "msize": "size",
                "keccak256": "hash",
                "calldataload": "calldataLoad",
                "calldatasize": "calldataSize",
                "calldatacopy": "calldataCopy",
                "returndatasize": "returnDataSize",
                "returndatacopy": "returnDataCopy",
            }[name]
            expression = f".memory .{tag}"
        elif name in {
            "call",
            "callcode",
            "delegatecall",
            "staticcall",
            "create",
            "create2",
        }:
            expression = f".external {opcode}"
        elif name in {"sload", "sstore", "tload", "tstore"}:
            expression = f".storage {'true' if name.startswith('t') else 'false'} {'true' if name.endswith('store') else 'false'}"
        elif name in {
            "address",
            "origin",
            "caller",
            "callvalue",
            "gasprice",
            "selfbalance",
            "balance",
            "extcodesize",
            "extcodehash",
            "extcodecopy",
            "blockhash",
            "coinbase",
            "timestamp",
            "number",
            "prevrandao",
            "gaslimit",
            "chainid",
            "basefee",
            "blobhash",
            "blobbasefee",
            "slotnum",
        }:
            expression = f".environment {opcode}"
        elif name in {"log0", "log1", "log2", "log3", "log4"}:
            expression = f".log {name[-1]}"
        else:
            raise Unsupported(f"unmodeled direct MIR opcode `{name}`")
        return f"some {opcode}", expression, args
    if kind in {"word", "ne", "select", "pointer_add"}:
        expression = {
            "word": ".word",
            "ne": ".notEqual",
            "select": ".select",
            "pointer_add": ".pointerAdd",
        }[kind]
    elif kind in {"insert_value", "extract_value"}:
        tag = "insertValue" if kind == "insert_value" else "extractValue"
        expression = (
            f".{tag} ({value_type(value['aggregate_type'])}) {number(value['index'])}"
        )
    elif kind == "make_slice":
        location = {"memory": 0, "calldata": 1, "returndata": 2}.get(value["location"])
        if location is None:
            raise Unsupported("unmodeled MIR slice location")
        expression = f".makeSlice {location}"
    elif kind in {
        "validate_storage_bytes",
        "load_storage_bytes",
        "store_storage_bytes",
        "clear_storage_words",
    }:
        expression = {
            "validate_storage_bytes": ".storageBytes .validate",
            "load_storage_bytes": ".storageBytes .load",
            "store_storage_bytes": ".storageBytes .store",
            "clear_storage_words": ".storageBytes .clear",
        }[kind]
    elif kind == "store_storage_bytes_literal":
        raw = value["bytes"]
        if not isinstance(raw, list) or len(raw) > 65536:
            raise Unsupported("MIR constant data limit of 64 KiB")
        expression = (
            ".storageBytes (.literal ["
            + ",".join(number(byte, 256) for byte in raw)
            + "])"
        )
    elif kind == "data_copy":
        raw = value["bytes"]
        if not isinstance(raw, list) or len(raw) > 65536:
            raise Unsupported("MIR constant data limit of 64 KiB")
        data = "[" + ",".join(number(byte, 256) for byte in raw) + "]"
        expression = f".dataCopy {data} {number(value['offset'])}"
    elif kind in {"mapping_slot", "mapping_slot_bytes", "array_data_slot"}:
        expression = {
            "mapping_slot": ".slot .mapping",
            "mapping_slot_bytes": ".slot .bytesKey",
            "array_data_slot": ".slot .arrayData",
        }[kind]
    elif kind == "array_element_slot":
        expression = f".slot (.arrayElement {number(value['stride'], 2**64)})"
    elif kind == "call":
        expression = f".call {number(value['function'])}"
    elif kind == "allocate":
        if type(value["zeroed"]) is not bool or type(value["checked"]) is not bool:
            raise Unsupported("invalid MIR allocation policy")
        expression = f".allocate {'true' if value['zeroed'] else 'false'} {'true' if value['checked'] else 'false'}"
    elif kind == "free_pointer":
        expression = ".freePointer"
    elif kind == "memory_zero":
        expression = ".memory .zero"
    elif kind == "low_bits":
        expression = f".lowBits {width(value['bits'])}"
    elif kind == "sign_extend":
        expression = f".signExtend {width(value['from'])} {width(value['to'])}"
    elif kind == "checked":
        tag = {
            "Add": "add",
            "Sub": "sub",
            "Mul": "mul",
            "Div": "div",
            "WrappingDiv": "wrappingDiv",
            "Rem": "rem",
            "Pow": "pow",
        }.get(value["operation"])
        if tag is None or type(value["signed"]) is not bool:
            raise Unsupported("unknown checked MIR operation")
        expression = (
            f".checkedPower {width(value['bits'])}"
            if tag == "pow"
            else f".checked .{tag} {width(value['bits'])}"
        )
        expression += " true" if value["signed"] else " false"
    elif kind == "check":
        if type(value["zero"]) is not bool:
            raise Unsupported("invalid MIR check predicate")
        panic = (
            "none"
            if value["panic"] is None
            else f"(some {number(value['panic'], 256)})"
        )
        expression = f".check {'true' if value['zero'] else 'false'} {panic}"
    else:
        raise Unsupported("unknown MIR model operation")
    return "none", expression, args


def block(value, definitions):
    phis = []
    for phi in value["phis"]:
        arms = (
            "["
            + ",".join(f"({number(b)},{operand(v)})" for b, v in phi["incoming"])
            + "]"
        )
        phis.append(f"⟨{number(phi['result'])},({value_type(phi['type'])}),{arms}⟩")
    instructions = []
    for inst in value["instructions"]:
        result = "none" if inst["result"] is None else f"some {number(inst['result'])}"
        ty = ".word 0" if inst["type"] is None else value_type(inst["type"])
        opcode, op, args = operation(inst["operation"], definitions)
        instructions.append(f"⟨({result}),({ty}),({opcode}),({op}),{operands(args)}⟩")
    term = value["terminator"]
    kind = term["kind"]
    if kind in {"stop", "invalid"}:
        end = "." + kind
    elif kind == "jump":
        end = f".jump {number(term['target'])}"
    elif kind == "branch":
        end = f".branch {operand(term['args'][0])} {number(term['then'])} {number(term['else'])}"
    elif kind == "switch":
        arms = (
            "["
            + ",".join(f"({operand(v)},{number(b)})" for v, b in term["cases"])
            + "]"
        )
        end = f".switch {operand(term['args'][0])} {number(term['default'])} {arms}"
    elif kind == "tail_call":
        end = f".tailCall {number(term['function'])} {operands(term['args'])}"
    elif kind == "return":
        end = f".returned {operands(term['args'])}"
    elif kind in {"revert", "returndata"}:
        end = f".{'reverted' if kind == 'revert' else kind} " + " ".join(
            map(operand, term["args"])
        )
    else:
        raise Unsupported("unknown MIR model terminator")
    return f"⟨[{','.join(phis)}],[{','.join(instructions)}],({end})⟩"


def function_body(value, definitions):
    params = "[" + ",".join(f"({value_type(ty)})" for ty in value["params"]) + "]"
    returns = "[" + ",".join(f"({value_type(ty)})" for ty in value["returns"]) + "]"
    blocks = "[" + ",".join(block(b, definitions) for b in value["blocks"]) + "]"
    return f"({number(value['id'])},⟨{params},{returns},{blocks}⟩)"


def translate(text, solar, version, fixture, timeout, function, replay=None):
    if fixture["gas_policy"] is not None:
        raise Unsupported("direct MIR state validation does not assign EVM gas costs")
    executable = replay or native.default(solar)
    exported = subprocess.run(
        [executable, "--mir-model"],
        input=json.dumps({"version": 1, "text": text, "function": function}),
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )
    try:
        response = json.loads(exported.stdout)
        if (
            exported.returncode
            or not isinstance(response, dict)
            or response.get("version") != 1
            or response.get("status") != "ok"
        ):
            raise Unsupported("MIR model: " + response.get("reason", "invalid export"))
        model = response["model"]
        definitions = {
            op["name"]: op for op in native.catalog(executable, timeout)["evmir"]
        }
        functions = (
            "["
            + ",".join(function_body(f, definitions) for f in model["functions"])
            + "]"
        )
        entry_id = number(model["entry_id"])
        ids = [number(f["id"]) for f in model["functions"]]
        if len(set(ids)) != len(ids) or entry_id not in ids:
            raise Unsupported("ambiguous or absent MIR entry function")
        params = model["params"]
        widths = [int(width(leaf["bits"])) for ty in params for leaf in leaf_types(ty)]
        return_widths = [
            int(width(leaf["bits"]))
            for ty in model["returns"]
            for leaf in leaf_types(ty)
        ]
        if len(widths) > 32 or len(return_widths) > 256:
            raise Unsupported("MIR boundary limit: 32 input and 256 return leaves")
        entry = model["entry"]
        if type(entry) is not bool:
            raise Unsupported("invalid MIR entry marker")
        if entry and any(a["transient"] for a in fixture["accounts"]):
            raise Unsupported(
                "MIR transaction entries start with empty transient storage"
            )
    except Unsupported:
        raise
    except (ValueError, KeyError, TypeError, IndexError) as error:
        raise Unsupported("invalid structured MIR export") from error
    available = [
        str(op["opcode"])
        for op in definitions.values()
        if op["forks"][version]["available"]
    ]
    env = world.environment_expression(fixture)
    external = any(
        inst["operation"]["kind"] == "opcode"
        and inst["operation"]["name"]
        in {"call", "callcode", "delegatecall", "staticcall", "create", "create2"}
        for func in model["functions"]
        for block in func["blocks"]
        for inst in block["instructions"]
    )
    if external and version == "amsterdam":
        raise Unsupported("Amsterdam MIR message gas is not modeled")
    config = world.configuration(
        solar, version, fixture, timeout, replay, root_is_mir=True
    ).replace("tvWorldConfig", "mirExternalConfig")
    external_config = "some mirExternalConfig"
    config += f"def mirWorldConfig : MIR.State.Config := ⟨.{version},{env},{world.context_expression(fixture)},[{','.join(available)}],{'true' if entry else 'false'},128,{external_config},{fixture['creation_gas']}⟩\n"
    boundary_inputs = []
    for index, ty in enumerate(params):
        for path, leaf in leaf_paths(ty):
            boundary_inputs.append(
                {
                    "input": f"arg{len(boundary_inputs)}",
                    "parameter": index,
                    "fields": list(path),
                    "type": leaf,
                }
            )
    inputs = [*fixture["symbols"], *(f"arg{i}" for i in range(len(widths)))]
    names = iter(f"arg{i}" for i in range(len(widths)))
    arguments = "[" + ",".join(f"({argument_value(ty, names)})" for ty in params) + "]"
    body = (
        f"let accounts : List Execution.Account := {world.accounts_expression(fixture)}\n"
        f"let journal : Execution.Journal := ⟨accounts,accounts,[{','.join(world.warm_addresses(fixture, version))}],[{','.join(f'({a},{k.removeprefix("$")})' for a, k in fixture['warm_slots'])}],[],0⟩\n"
        f"let memory : List Execution.Byte := {fixture['memory']['data']} ++ List.replicate {fixture['memory']['size'] - len(fixture['memory']['data'])} 0\n"
        f"MIR.State.execute {fixture['fuel']} mirWorldConfig {functions} {entry_id} ⟨journal,memory,{arguments},[],[],0⟩"
    )
    return Program(
        (
            "world-entry",
            "mir",
            model["name"],
            json.dumps(widths),
            json.dumps(return_widths),
            str(entry),
            fixture["fixture_sha256"],
        ),
        inputs,
        "MIR.State.Run",
        body,
        (256,) * len(fixture["symbols"]) + tuple(widths),
        canonical_ir=text,
        selector=model["name"],
        evm_version=version,
        scope="mir-state",
        world={
            **fixture,
            "configuration": config,
            "direct_mir": True,
            "entry": entry,
            "boundary": {
                "inputs": boundary_inputs,
                "params": [leaf for ty in params for leaf in leaf_types(ty)],
                "returns": [leaf for ty in model["returns"] for leaf in leaf_types(ty)],
            },
        },
    )
