"""Explicit code-image, relocation and placement inputs for direct MIR windows."""

import json

from .common import Unsupported, literal


def read(value, memory):
    from .world import _bytes, _keys, _number

    _keys(
        value,
        {
            "code",
            "functions",
            "constructor",
            "data",
            "immutable_slots",
            "immutable_values",
            "libraries",
            "gas_before",
        },
        "mir_layout",
    )
    result: dict = {
        "code": None,
        "functions": {},
        "constructor": None,
        "data": {},
        "immutable_slots": {},
        "immutable_values": {},
        "libraries": {},
        "gas_before": None,
    }
    if value.get("code") is not None:
        result["code"] = _bytes(value["code"])
    for group in (
        "functions",
        "data",
        "immutable_slots",
        "immutable_values",
        "libraries",
    ):
        table = value.get(group, {})
        if not isinstance(table, dict) or len(table) > 128:
            raise Unsupported("MIR layout tables require at most 128 bindings")
        for key, item in table.items():
            if not isinstance(key, str) or (
                group != "functions"
                and (
                    not key.isascii()
                    or not key.isdecimal()
                    or int(key) >= 2**32
                    or str(int(key)) != key
                )
            ):
                raise Unsupported("invalid MIR layout binding key")
            if group == "functions":
                if (
                    len(key) > 256
                    or not key.isascii()
                    or any(ord(c) < 32 or ord(c) == 127 for c in key)
                ):
                    raise Unsupported("invalid MIR placement function name")
                _keys(item, {"heap_floor", "frame_base"}, "MIR function placement")
                fields = {name: None for name in ("heap_floor", "frame_base")}
                for name, address in item.items():
                    address = int(_number(address, 64))
                    if address < 128 or address % 32:
                        raise Unsupported(
                            "MIR placements must be aligned and at least 128"
                        )
                    fields[name] = address
                result[group][key] = fields
            elif group == "data":
                _keys(item, {"offset", "size"}, "linked data extent")
                offset, size = (
                    int(_number(item[name], 32)) for name in ("offset", "size")
                )
                if result["code"] is None or offset + size > len(result["code"]):
                    raise Unsupported("linked data is outside the code image")
                result[group][key] = {"offset": offset, "size": size}
            elif group == "immutable_slots":
                address = int(_number(item, 64))
                if address < 128 or address > 65504 or address % 32:
                    raise Unsupported("invalid immutable staging slot")
                result[group][key] = address
            else:
                result[group][key] = _number(item, 160 if group == "libraries" else 256)
    if set(result["immutable_slots"]) & set(result["immutable_values"]):
        raise Unsupported("ambiguous immutable staging and runtime value")
    if value.get("constructor") is not None:
        item = value["constructor"]
        _keys(item, {"offset", "size"}, "constructor range")
        offset, size = (int(_number(item[name], 32)) for name in ("offset", "size"))
        if offset + size > memory["size"]:
            raise Unsupported("constructor range is outside initial memory")
        result["constructor"] = {"offset": offset, "size": size}
    if value.get("gas_before") is not None:
        gas = int(_number(value["gas_before"], 64))
        if gas > 30000000:
            raise Unsupported("MIR gas window exceeds 30000000")
        result["gas_before"] = gas
    pointer = int.from_bytes(bytes(memory["data"][64:96]).ljust(32, b"\0"), "big")
    if any(
        fields["heap_floor"] is not None and fields["heap_floor"] > pointer
        for fields in result["functions"].values()
    ):
        raise Unsupported("heap floor exceeds the explicit initial FMP")
    return result


def expression(layout):
    def option(value, render=str):
        return "none" if value is None else f"some ({render(value)})"

    def table(name, render):
        return (
            "["
            + ",".join(
                f"({key},{render(value)})" for key, value in layout[name].items()
            )
            + "]"
        )

    code = option(layout["code"], str)
    functions = (
        "["
        + ",".join(
            f"({json.dumps(name)},⟨{option(value['heap_floor'], lambda x: literal(str(x)))},{option(value['frame_base'], lambda x: literal(str(x)))}⟩)"
            for name, value in layout["functions"].items()
        )
        + "]"
    )
    constructor = option(
        layout["constructor"], lambda r: f"({r['offset']},{r['size']})"
    )
    data = table("data", lambda r: f"({r['offset']},{r['size']})")
    slots = table("immutable_slots", str)
    values = table("immutable_values", literal)
    libraries = table("libraries", literal)
    gas = option(layout["gas_before"])
    return (
        f"⟨{code},{functions},{constructor},{data},{slots},{values},{libraries},{gas}⟩"
    )
