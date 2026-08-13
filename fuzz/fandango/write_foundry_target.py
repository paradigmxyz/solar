#!/usr/bin/env python3
"""Write Foundry fuzz targets for generated runtime harnesses."""

from __future__ import annotations

import argparse
import pathlib
import re

import evm_runtime as evm
import symbolic_differential as symbolic


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=pathlib.Path, required=True)
    parser.add_argument("--out-dir", type=pathlib.Path, required=True)
    parser.add_argument("--solc-runtime")
    parser.add_argument("--solar-runtime")
    args = parser.parse_args()

    write_target(args.source, args.out_dir, args.solc_runtime, args.solar_runtime)
    print(args.out_dir)
    return 0


def write_target(
    source: pathlib.Path,
    out_dir: pathlib.Path,
    solc_runtime: str | None = None,
    solar_runtime: str | None = None,
) -> None:
    src_dir = out_dir / "src"
    test_dir = out_dir / "test"
    src_dir.mkdir(parents=True, exist_ok=True)
    test_dir.mkdir(parents=True, exist_ok=True)

    (src_dir / "FandangoRuntime.sol").write_text(source.read_text())
    if solc_runtime is None and solar_runtime is None:
        test_source = _SELF_TEST_SOURCE
    elif solc_runtime is not None and solar_runtime is not None:
        test_source = _differential_test_source(solc_runtime, solar_runtime)
    else:
        raise ValueError("solc and solar runtimes must be provided together")

    (test_dir / "FandangoRuntime.t.sol").write_text(test_source)
    (out_dir / "foundry.toml").write_text(_FOUNDRY_TOML)


def write_symbolic_target(
    out_dir: pathlib.Path,
    solc_runtime: str,
    solar_runtime: str,
    function: dict[str, object],
    max_returndata_bytes: int,
    evm_version: str,
    dynamic_lengths: tuple[int, ...] = symbolic.DEFAULT_SYMBOLIC_DYNAMIC_LENGTHS,
    exploration_order: str = "bfs",
    storage_layout: str = "solidity",
    input_lengths: dict[str, tuple[int, ...]] | None = None,
    stateful: bool = False,
) -> None:
    """Write one bounded symbolic differential target."""
    if max_returndata_bytes <= 0:
        raise ValueError("max returndata bytes must be positive")
    if (
        not dynamic_lengths
        or any(
            not isinstance(length, int)
            or isinstance(length, bool)
            or length < 0
            or length > symbolic.MAX_SYMBOLIC_DYNAMIC_LENGTH
            for length in dynamic_lengths
        )
        or len(set(dynamic_lengths)) != len(dynamic_lengths)
    ):
        raise ValueError(
            "dynamic lengths must be unique integers from 0 through "
            f"{symbolic.MAX_SYMBOLIC_DYNAMIC_LENGTH}"
        )
    if exploration_order not in {"bfs", "dfs"}:
        raise ValueError("symbolic exploration order must be bfs or dfs")
    if storage_layout not in {"solidity", "zero_init"}:
        raise ValueError("symbolic storage layout must be solidity or zero_init")
    input_lengths = input_lengths or {}
    if any(
        not re.fullmatch(r"arg[0-9]+", name)
        or not lengths
        or any(
            not isinstance(length, int)
            or isinstance(length, bool)
            or length < 0
            or length > symbolic.MAX_SYMBOLIC_DYNAMIC_LENGTH
            for length in lengths
        )
        or len(set(lengths)) != len(lengths)
        for name, lengths in input_lengths.items()
    ):
        raise ValueError("symbolic input lengths are malformed")
    src_dir = out_dir / "src"
    test_dir = out_dir / "test"
    src_dir.mkdir(parents=True, exist_ok=True)
    test_dir.mkdir(parents=True, exist_ok=True)

    input_types = list(function["inputs"])
    abi = function.get("abi")
    abi_inputs = (
        abi.get("inputs")
        if isinstance(abi, dict) and isinstance(abi.get("inputs"), list)
        else [{"type": abi_type} for abi_type in input_types]
    )
    struct_declarations, parameter_declarations = (
        symbolic.solidity_symbolic_parameters(abi_inputs)
    )
    declarations = ", ".join(parameter_declarations)
    arguments = ", ".join(f"arg{index}" for index in range(len(input_types)))
    encode = f"abi.encodeWithSelector(TARGET_SELECTOR{', ' if arguments else ''}{arguments})"
    word_checks = "\n".join(
        "        "
        f"if (retA.length > {offset}) "
        f"assert(_word(retA, {offset}) == _word(retB, {offset}));"
        for offset in range(0, max_returndata_bytes, 32)
    )
    template = (
        _STATEFUL_SYMBOLIC_DIFFERENTIAL_TEST_SOURCE_TEMPLATE
        if stateful
        else _SYMBOLIC_DIFFERENTIAL_TEST_SOURCE_TEMPLATE
    )
    test_source = template.format(
        solc_runtime=_hex_literal(solc_runtime),
        solar_runtime=_hex_literal(solar_runtime),
        selector=function["selector"],
        test_name=function["test"],
        struct_declarations=struct_declarations,
        declarations=declarations,
        encode=encode,
        max_returndata_bytes=max_returndata_bytes,
        word_checks=word_checks,
        router_address=evm.SYMBOLIC_ROUTER_ADDRESS,
        solc_runtime_address=evm.SYMBOLIC_SOLC_RUNTIME_ADDRESS,
        solar_runtime_address=evm.SYMBOLIC_SOLAR_RUNTIME_ADDRESS,
        state_mirror_address=evm.SYMBOLIC_STATE_MIRROR_ADDRESS,
    )
    (test_dir / "SymbolicDifferential.t.sol").write_text(test_source)
    (out_dir / "foundry.toml").write_text(
        _SYMBOLIC_FOUNDRY_TOML.format(
            evm_version=evm_version,
            dynamic_lengths=", ".join(str(length) for length in dynamic_lengths),
            exploration_order=exploration_order,
            storage_layout=storage_layout,
            input_lengths=", ".join(
                f"{name} = [{', '.join(str(length) for length in lengths)}]"
                for name, lengths in sorted(input_lengths.items())
            ),
        )
    )


_FOUNDRY_TOML = """\
[profile.default]
src = "src"
test = "test"
out = "out"
libs = []
optimizer = true
optimizer_runs = 200
via_ir = true

[fuzz]
runs = 64
max_test_rejects = 65536
"""


_SYMBOLIC_FOUNDRY_TOML = """\
[profile.default]
src = "src"
test = "test"
out = "out"
cache_path = "cache"
libs = []
optimizer = true
optimizer_runs = 200
via_ir = true
evm_version = "{evm_version}"
code_size_limit = 1000000

[symbolic]
exploration_order = "{exploration_order}"
storage_layout = "{storage_layout}"
dynamic_lengths = {{ {input_lengths} }}
default_array_lengths = [{dynamic_lengths}]
default_bytes_lengths = [{dynamic_lengths}]
"""


_SELF_TEST_SOURCE = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

import {FandangoRuntime} from "../src/FandangoRuntime.sol";

contract FandangoRuntimeTest {
    FandangoRuntime target;

    function setUp() public {
        target = new FandangoRuntime();
    }

    function testFuzzRun(uint256 seed, uint256 a, uint256 b, bytes memory data) public {
        target.setup(seed);
        uint256 result = target.run(a, b, data);
        (uint256 value, uint256 stored) = target.observe(a);

        assert(value == result);
        assert(stored == result);
    }
}
"""


def _differential_test_source(solc_runtime: str, solar_runtime: str) -> str:
    return _DIFFERENTIAL_TEST_SOURCE_TEMPLATE.format(
        solc_runtime=_hex_literal(solc_runtime),
        solar_runtime=_hex_literal(solar_runtime),
    )


def _hex_literal(value: str) -> str:
    value = value.removeprefix("0x")
    if len(value) % 2 != 0:
        raise ValueError("runtime bytecode has an odd number of hex digits")
    int(value or "0", 16)
    return value


_DIFFERENTIAL_TEST_SOURCE_TEMPLATE = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {{
    enum AccountAccessKind {{
        Call,
        DelegateCall,
        CallCode,
        StaticCall,
        Create,
        SelfDestruct,
        Resume,
        Balance,
        Extcodesize,
        Extcodehash,
        Extcodecopy
    }}

    struct ChainInfo {{
        uint256 forkId;
        uint256 chainId;
    }}

    struct StorageAccess {{
        address account;
        bytes32 slot;
        bool isWrite;
        bytes32 previousValue;
        bytes32 newValue;
        bool reverted;
    }}

    struct AccountAccess {{
        ChainInfo chainInfo;
        AccountAccessKind kind;
        address account;
        address accessor;
        bool initialized;
        uint256 oldBalance;
        uint256 newBalance;
        bytes deployedCode;
        uint256 value;
        bytes data;
        bool reverted;
        StorageAccess[] storageAccesses;
        uint64 depth;
    }}

    struct Log {{
        bytes32[] topics;
        bytes data;
        address emitter;
    }}

    function assume(bool condition) external pure;
    function deal(address account, uint256 newBalance) external;
    function etch(address target, bytes calldata newRuntimeBytecode) external;
    function getRecordedLogs() external returns (Log[] memory logs);
    function prank(address msgSender) external;
    function recordLogs() external;
    function roll(uint256 newHeight) external;
    function startStateDiffRecording() external;
    function stopAndReturnStateDiff() external returns (AccountAccess[] memory accountAccesses);
    function warp(uint256 newTimestamp) external;
}}

contract FandangoRuntimeDifferentialTest {{
    Vm internal constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    address internal constant SOLC_IMPL = address(0x1000000000000000000000000000000000000001);
    address internal constant SOLAR_IMPL = address(0x1000000000000000000000000000000000000002);
    address internal constant NORMALIZED_IMPL = address(0x000000000000000000000000000000000000dEaD);

    bytes internal constant SOLC_RUNTIME = hex"{solc_runtime}";
    bytes internal constant SOLAR_RUNTIME = hex"{solar_runtime}";

    function setUp() public {{
        vm.etch(SOLC_IMPL, SOLC_RUNTIME);
        vm.etch(SOLAR_IMPL, SOLAR_RUNTIME);
    }}

    function testFuzz_Differential_Harness(
        uint256 seed,
        uint256 a,
        uint256 b,
        bytes calldata data,
        address caller,
        uint256 timestamp,
        uint256 blockNumber
    ) public {{
        vm.assume(data.length <= 4096);
        _compare(abi.encodeWithSignature("setup(uint256)", seed), caller, 0, timestamp, blockNumber);
        _compare(
            abi.encodeWithSignature("run(uint256,uint256,bytes)", a, b, data),
            caller,
            0,
            timestamp,
            blockNumber
        );
        uint256 key;
        unchecked {{
            key = (a + b) & 7;
        }}
        _compare(abi.encodeWithSignature("observe(uint256)", key), caller, 0, timestamp, blockNumber);
    }}

    function testFuzz_Differential_ArbitraryCalldata(
        bytes calldata callData,
        address caller,
        uint256 timestamp,
        uint256 blockNumber
    ) public {{
        vm.assume(callData.length <= 512);
        _compare(callData, caller, 0, timestamp, blockNumber);
    }}

    function _compare(
        bytes memory callData,
        address caller,
        uint256 value,
        uint256 timestamp,
        uint256 blockNumber
    ) internal {{
        _assumeCaller(caller);
        uint256 ts = _bound(timestamp, 1, type(uint64).max);
        uint256 bn = _bound(blockNumber, 1, type(uint64).max);

        vm.deal(caller, value);
        vm.warp(ts);
        vm.roll(bn);
        vm.startStateDiffRecording();
        vm.recordLogs();
        vm.prank(caller);
        (bool okA, bytes memory retA) = SOLC_IMPL.call{{value: value}}(callData);
        Vm.AccountAccess[] memory diffA = vm.stopAndReturnStateDiff();
        Vm.Log[] memory logsA = vm.getRecordedLogs();

        vm.deal(caller, value);
        vm.warp(ts);
        vm.roll(bn);
        vm.startStateDiffRecording();
        vm.recordLogs();
        vm.prank(caller);
        (bool okB, bytes memory retB) = SOLAR_IMPL.call{{value: value}}(callData);
        Vm.AccountAccess[] memory diffB = vm.stopAndReturnStateDiff();
        Vm.Log[] memory logsB = vm.getRecordedLogs();

        if (okA != okB) revert("success mismatch");
        if (keccak256(retA) != keccak256(retB)) revert("returndata mismatch");
        if (_logsHash(logsA, SOLC_IMPL) != _logsHash(logsB, SOLAR_IMPL)) revert("logs mismatch");
        if (_diffHash(diffA, SOLC_IMPL) != _diffHash(diffB, SOLAR_IMPL)) {{
            revert("state diff mismatch");
        }}
    }}

    function _assumeCaller(address caller) internal view {{
        vm.assume(caller != address(0));
        vm.assume(caller != address(vm));
        vm.assume(caller != address(this));
        vm.assume(caller != SOLC_IMPL);
        vm.assume(caller != SOLAR_IMPL);
    }}

    function _logsHash(Vm.Log[] memory logs, address impl) internal pure returns (bytes32 digest) {{
        for (uint256 i = 0; i < logs.length; ++i) {{
            digest = keccak256(abi.encode(
                digest,
                _normalize(logs[i].emitter, impl),
                logs[i].topics,
                logs[i].data
            ));
        }}
    }}

    function _diffHash(
        Vm.AccountAccess[] memory accesses,
        address impl
    ) internal pure returns (bytes32 digest) {{
        for (uint256 i = 0; i < accesses.length; ++i) {{
            Vm.AccountAccess memory access = accesses[i];
            for (uint256 j = 0; j < access.storageAccesses.length; ++j) {{
                Vm.StorageAccess memory storageAccess = access.storageAccesses[j];
                if (!storageAccess.isWrite || storageAccess.reverted) continue;
                if (_hasLaterStorageWrite(accesses, impl, i, j)) continue;
                digest = _xor(digest, keccak256(abi.encode(
                    "storage",
                    _normalize(storageAccess.account, impl),
                    storageAccess.slot,
                    storageAccess.newValue
                )));
            }}
            bool balanceChanged = access.oldBalance != access.newBalance;
            bool lifetimeChanged = access.kind == Vm.AccountAccessKind.Create
                || access.kind == Vm.AccountAccessKind.SelfDestruct;
            if (balanceChanged) {{
                digest = _xor(digest, keccak256(abi.encode(
                    "balance",
                    access.chainInfo.forkId,
                    access.chainInfo.chainId,
                    _normalize(access.account, impl),
                    access.newBalance
                )));
            }}
            if (lifetimeChanged) {{
                digest = _xor(digest, keccak256(abi.encode(
                    "lifetime",
                    access.chainInfo.forkId,
                    access.chainInfo.chainId,
                    access.kind,
                    _normalize(access.account, impl),
                    keccak256(access.deployedCode)
                )));
            }}
        }}
    }}

    function _hasLaterStorageWrite(
        Vm.AccountAccess[] memory accesses,
        address impl,
        uint256 accessIndex,
        uint256 storageIndex
    ) internal pure returns (bool) {{
        Vm.StorageAccess memory current = accesses[accessIndex].storageAccesses[storageIndex];
        address currentAccount = _normalize(current.account, impl);
        for (uint256 i = accessIndex; i < accesses.length; ++i) {{
            Vm.StorageAccess[] memory writes = accesses[i].storageAccesses;
            uint256 start = i == accessIndex ? storageIndex + 1 : 0;
            for (uint256 j = start; j < writes.length; ++j) {{
                Vm.StorageAccess memory candidate = writes[j];
                if (
                    candidate.isWrite
                        && !candidate.reverted
                        && _normalize(candidate.account, impl) == currentAccount
                        && candidate.slot == current.slot
                ) {{
                    return true;
                }}
            }}
        }}
        return false;
    }}

    function _xor(bytes32 a, bytes32 b) internal pure returns (bytes32) {{
        return bytes32(uint256(a) ^ uint256(b));
    }}

    function _normalize(address value, address impl) internal pure returns (address) {{
        return value == impl ? NORMALIZED_IMPL : value;
    }}

    function _bound(uint256 value, uint256 min, uint256 max) internal pure returns (uint256) {{
        uint256 size = max - min + 1;
        return min + (value % size);
    }}
}}
"""


_STATEFUL_SYMBOLIC_DIFFERENTIAL_TEST_SOURCE_TEMPLATE = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {{
    struct Log {{
        bytes32[] topics;
        bytes data;
        address emitter;
    }}

    function accesses(address target)
        external
        view
        returns (bytes32[] memory readSlots, bytes32[] memory writeSlots);
    function etch(address target, bytes calldata newRuntimeBytecode) external;
    function getRecordedLogs() external returns (Log[] memory logs);
    function load(address target, bytes32 slot) external view returns (bytes32 data);
    function record() external;
    function recordLogs() external;
    function stopRecord() external;
    function store(address target, bytes32 slot, bytes32 value) external;
}}

contract RuntimeRouter {{
    fallback() external payable {{
        assembly ("memory-safe") {{
            if lt(calldatasize(), 20) {{ revert(0, 0) }}
            let target := shr(96, calldataload(0))
            let targetCalldataSize := sub(calldatasize(), 20)
            calldatacopy(0, 20, targetCalldataSize)
            let ok := delegatecall(gas(), target, 0, targetCalldataSize, 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch ok
            case 0 {{ revert(0, returndatasize()) }}
            default {{ return(0, returndatasize()) }}
        }}
    }}
}}

contract SymbolicDifferentialTest {{
{struct_declarations}
    Vm internal constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    RuntimeRouter internal constant ROUTER =
        RuntimeRouter(payable({router_address}));
    address internal constant SOLC_IMPLEMENTATION =
        address({solc_runtime_address});
    address internal constant SOLAR_IMPLEMENTATION =
        address({solar_runtime_address});
    address internal constant STATE_MIRROR =
        address({state_mirror_address});
    bytes4 internal constant TARGET_SELECTOR = bytes4({selector});
    uint256 internal constant MAX_RETURNDATA_BYTES = {max_returndata_bytes};

    bytes internal constant SOLC_RUNTIME = hex"{solc_runtime}";
    bytes internal constant SOLAR_RUNTIME = hex"{solar_runtime}";

    function setUp() public {{
        vm.etch(address(ROUTER), type(RuntimeRouter).runtimeCode);
        vm.etch(SOLC_IMPLEMENTATION, SOLC_RUNTIME);
        vm.etch(SOLAR_IMPLEMENTATION, SOLAR_RUNTIME);
    }}

    function {test_name}({declarations}) public {{
        bytes memory callData = {encode};

        vm.record();
        vm.recordLogs();
        (bool okA, bytes memory retA) = _routedCall(SOLC_IMPLEMENTATION, callData);
        (, bytes32[] memory writesA) = vm.accesses(address(ROUTER));
        vm.stopRecord();
        Vm.Log[] memory logsA = vm.getRecordedLogs();

        for (uint256 i; i < writesA.length; ++i) {{
            bytes32 valueA = vm.load(address(ROUTER), writesA[i]);
            vm.store(STATE_MIRROR, writesA[i], valueA);
            vm.store(address(ROUTER), writesA[i], bytes32(0));
        }}

        vm.record();
        vm.recordLogs();
        (bool okB, bytes memory retB) = _routedCall(SOLAR_IMPLEMENTATION, callData);
        (, bytes32[] memory writesB) = vm.accesses(address(ROUTER));
        vm.stopRecord();
        Vm.Log[] memory logsB = vm.getRecordedLogs();

        assert(okA == okB);
        assert(retA.length == retB.length);

        // This is an explicit soundness sentinel, not a mismatch claim. If a
        // candidate reaches it, the runner independently replays both calls.
        if (retA.length > MAX_RETURNDATA_BYTES) assert(false);
{word_checks}
        assert(keccak256(abi.encode(logsA)) == keccak256(abi.encode(logsB)));

        for (uint256 i; i < writesA.length; ++i) {{
            assert(
                vm.load(STATE_MIRROR, writesA[i])
                    == vm.load(address(ROUTER), writesA[i])
            );
        }}
        for (uint256 i; i < writesB.length; ++i) {{
            assert(
                vm.load(STATE_MIRROR, writesB[i])
                    == vm.load(address(ROUTER), writesB[i])
            );
        }}
    }}

    function _routedCall(
        address target,
        bytes memory callData
    ) internal returns (bool ok, bytes memory result) {{
        return address(ROUTER).call(abi.encodePacked(target, callData));
    }}

    function _word(bytes memory value, uint256 offset) internal pure returns (uint256 result) {{
        assembly ("memory-safe") {{
            result := mload(add(add(value, 0x20), offset))
        }}
        uint256 remaining = value.length - offset;
        if (remaining < 32) result >>= (32 - remaining) * 8;
    }}
}}
"""


_SYMBOLIC_DIFFERENTIAL_TEST_SOURCE_TEMPLATE = """\
// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

interface Vm {{
    function etch(address target, bytes calldata newRuntimeBytecode) external;
}}

contract RuntimeRouter {{
    fallback() external payable {{
        assembly ("memory-safe") {{
            if lt(calldatasize(), 20) {{ revert(0, 0) }}
            let target := shr(96, calldataload(0))
            let targetCalldataSize := sub(calldatasize(), 20)
            calldatacopy(0, 20, targetCalldataSize)
            let ok := delegatecall(gas(), target, 0, targetCalldataSize, 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch ok
            case 0 {{ revert(0, returndatasize()) }}
            default {{ return(0, returndatasize()) }}
        }}
    }}
}}

contract SymbolicDifferentialTest {{
{struct_declarations}
    Vm internal constant vm = Vm(0x7109709ECfa91a80626fF3989D68f67F5b1DD12D);

    RuntimeRouter internal constant ROUTER =
        RuntimeRouter(payable({router_address}));
    address internal constant SOLC_IMPLEMENTATION =
        address({solc_runtime_address});
    address internal constant SOLAR_IMPLEMENTATION =
        address({solar_runtime_address});
    bytes4 internal constant TARGET_SELECTOR = bytes4({selector});
    uint256 internal constant MAX_RETURNDATA_BYTES = {max_returndata_bytes};

    bytes internal constant SOLC_RUNTIME = hex"{solc_runtime}";
    bytes internal constant SOLAR_RUNTIME = hex"{solar_runtime}";

    function setUp() public {{
        vm.etch(address(ROUTER), type(RuntimeRouter).runtimeCode);
        vm.etch(SOLC_IMPLEMENTATION, SOLC_RUNTIME);
        vm.etch(SOLAR_IMPLEMENTATION, SOLAR_RUNTIME);
    }}

    function {test_name}({declarations}) public {{
        bytes memory callData = {encode};
        _warmRouter();
        (bool okA, bytes memory retA) = _routedStaticCall(SOLC_IMPLEMENTATION, callData);
        (bool okB, bytes memory retB) = _routedStaticCall(SOLAR_IMPLEMENTATION, callData);

        assert(okA == okB);
        assert(retA.length == retB.length);

        // This is an explicit soundness sentinel, not a mismatch claim. If a
        // candidate reaches it, the runner independently replays both calls.
        // Equal concrete outcomes are classified as incomplete and tell the
        // caller to raise --max-returndata-bytes.
        if (retA.length > MAX_RETURNDATA_BYTES) assert(false);
{word_checks}
    }}

    function _warmRouter() internal {{
        (bool ok,) = address(ROUTER).staticcall("");
        assert(!ok);
    }}

    function _routedStaticCall(
        address target,
        bytes memory callData
    ) internal returns (bool ok, bytes memory result) {{
        return address(ROUTER).staticcall(abi.encodePacked(target, callData));
    }}

    function _word(bytes memory value, uint256 offset) internal pure returns (uint256 result) {{
        assembly ("memory-safe") {{
            result := mload(add(add(value, 0x20), offset))
        }}
        uint256 remaining = value.length - offset;
        if (remaining < 32) result >>= (32 - remaining) * 8;
    }}
}}
"""


if __name__ == "__main__":
    raise SystemExit(main())
