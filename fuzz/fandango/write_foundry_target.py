#!/usr/bin/env python3
"""Write Foundry fuzz targets for generated runtime harnesses."""

from __future__ import annotations

import argparse
import pathlib


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


if __name__ == "__main__":
    raise SystemExit(main())
