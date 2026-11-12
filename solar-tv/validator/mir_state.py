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


def operand(value):
    bits = width(value["type"]["bits"])
    kind = value["kind"]
    if kind == "constant":
        atom = f".constant ⟨{bits},{literal(value['value'], int(bits))}⟩"
    elif kind in {"arg", "local"}:
        atom = f".{'argument' if kind == 'arg' else 'local'} {number(value['index'])}"
    elif kind == "undefined":
        atom = ".undefined"
    else:
        raise Unsupported("unknown MIR model operand")
    return f"⟨({atom}),{bits}⟩"


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
        phis.append(f"⟨{number(phi['result'])},{width(phi['type']['bits'])},{arms}⟩")
    instructions = []
    for inst in value["instructions"]:
        result = "none" if inst["result"] is None else f"some {number(inst['result'])}"
        bits = "0" if inst["type"] is None else width(inst["type"]["bits"])
        opcode, op, args = operation(inst["operation"], definitions)
        instructions.append(f"⟨({result}),{bits},({opcode}),({op}),{operands(args)}⟩")
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
    params = "[" + ",".join(width(ty["bits"]) for ty in value["params"]) + "]"
    returns = "[" + ",".join(width(ty["bits"]) for ty in value["returns"]) + "]"
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
        widths = [int(width(p["bits"])) for p in params]
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
    inputs = [*fixture["symbols"], *(f"arg{i}" for i in range(len(params)))]
    arguments = (
        "[" + ",".join(f"⟨{bits},arg{i}⟩" for i, bits in enumerate(widths)) + "]"
    )
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
            json.dumps(params, sort_keys=True),
            json.dumps(model["returns"], sort_keys=True),
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
        world={**fixture, "configuration": config, "direct_mir": True, "entry": entry},
    )
