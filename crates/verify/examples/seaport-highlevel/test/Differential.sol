// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { VmSafe as Vm } from "forge-std/Vm.sol";

import { SeaportVariants } from "./Deploy.sol";

/// Runs one call against two Seaport runtimes installed in turn at the same
/// address, from the same state, and requires the same observable behaviour:
/// success, return or revert data, logs, and the ordered calls, creations,
/// value transfers and storage writes the state-diff recorder sees. Gas is
/// not compared. The second runtime's effects are the ones that persist.
abstract contract Differential is SeaportVariants {
    address internal seaport;
    bytes internal codeA;
    bytes internal codeB;

    struct Outcome {
        bool ok;
        bytes ret;
        Vm.Log[] logs;
        Vm.AccountAccess[] accesses;
    }

    function _setUpVariants(string memory a, string memory b) internal {
        _deployController();
        (address at, bytes memory ca) = _deployAt(a);
        (address at2, bytes memory cb) = _deployAt(b);
        require(at == at2, "variants landed at different addresses");
        seaport = at;
        codeA = ca;
        codeB = cb;
        vm.etch(seaport, codeA);
    }

    function _run(bytes memory code, address caller, uint256 value, bytes memory data)
        internal
        returns (Outcome memory o)
    {
        vm.etch(seaport, code);
        vm.deal(caller, caller.balance + value);
        vm.recordLogs();
        vm.startStateDiffRecording();
        vm.prank(caller);
        (o.ok, o.ret) = seaport.call{ value: value }(data);
        o.accesses = vm.stopAndReturnStateDiff();
        o.logs = vm.getRecordedLogs();
    }

    /// Calls both runtimes and compares; the state after B persists.
    function _diff(address caller, uint256 value, bytes memory data)
        internal
        returns (Outcome memory b)
    {
        uint256 snap = vm.snapshotState();
        Outcome memory a = _run(codeA, caller, value, data);
        vm.revertToState(snap);
        b = _run(codeB, caller, value, data);
        _assertSame(a, b);
    }

    function _assertSame(Outcome memory a, Outcome memory b) internal pure {
        assertEq(a.ok, b.ok, "success");
        assertEq(a.ret, b.ret, a.ok ? "return data" : "revert data");
        assertEq(a.logs.length, b.logs.length, "log count");
        for (uint256 i; i < a.logs.length; ++i) {
            assertEq(a.logs[i].emitter, b.logs[i].emitter, "log emitter");
            assertEq(a.logs[i].topics, b.logs[i].topics, "log topics");
            assertEq(a.logs[i].data, b.logs[i].data, "log data");
        }
        Vm.AccountAccess[] memory ea = _effects(a.accesses);
        Vm.AccountAccess[] memory eb = _effects(b.accesses);
        assertEq(ea.length, eb.length, "call count");
        for (uint256 i; i < ea.length; ++i) {
            assertEq(uint256(ea[i].kind), uint256(eb[i].kind), "call kind");
            assertEq(ea[i].accessor, eb[i].accessor, "caller of call");
            assertEq(ea[i].account, eb[i].account, "call target");
            assertEq(ea[i].value, eb[i].value, "call value");
            assertEq(ea[i].reverted, eb[i].reverted, "call reverted");
            assertEq(ea[i].depth, eb[i].depth, "call depth");
            // The top-level call's data is the input itself; a creation's
            // data is init code, which may differ only for the TLOAD probe.
            if (ea[i].kind != _CREATE) {
                assertEq(ea[i].data, eb[i].data, "call data");
            }
            _assertSameWrites(ea[i].storageAccesses, eb[i].storageAccesses);
        }
    }

    Vm.AccountAccessKind internal constant _CREATE = Vm.AccountAccessKind.Create;

    /// Calls, creations and self-destructs, in order; reads such as
    /// BALANCE or EXTCODESIZE are implementation details.
    function _effects(Vm.AccountAccess[] memory all)
        internal
        pure
        returns (Vm.AccountAccess[] memory out)
    {
        uint256 n;
        out = new Vm.AccountAccess[](all.length);
        for (uint256 i; i < all.length; ++i) {
            Vm.AccountAccessKind k = all[i].kind;
            if (
                k == Vm.AccountAccessKind.Call
                    || k == Vm.AccountAccessKind.StaticCall
                    || k == Vm.AccountAccessKind.DelegateCall
                    || k == Vm.AccountAccessKind.CallCode
                    || k == _CREATE
                    || k == Vm.AccountAccessKind.SelfDestruct
            ) {
                out[n++] = all[i];
            }
        }
        assembly ("memory-safe") {
            mstore(out, n)
        }
    }

    /// Storage writes in order, with the slot, value and whether the
    /// frame that made it reverted.
    function _assertSameWrites(
        Vm.StorageAccess[] memory a,
        Vm.StorageAccess[] memory b
    ) internal pure {
        uint256 j;
        uint256 i;
        while (true) {
            while (i < a.length && !a[i].isWrite) ++i;
            while (j < b.length && !b[j].isWrite) ++j;
            if (i == a.length || j == b.length) break;
            assertEq(a[i].account, b[j].account, "write account");
            assertEq(a[i].slot, b[j].slot, "write slot");
            assertEq(a[i].newValue, b[j].newValue, "write value");
            assertEq(a[i].reverted, b[j].reverted, "write reverted");
            ++i;
            ++j;
        }
        assertTrue(i == a.length && j == b.length, "write count");
    }
}
