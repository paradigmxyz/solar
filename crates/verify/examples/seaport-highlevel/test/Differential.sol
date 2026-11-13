// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import { VmSafe as Vm } from "forge-std/Vm.sol";

import { SeaportVariants } from "./Deploy.sol";

/// Runs one call against two Seaport runtimes installed in turn at the same
/// address, from the same state, and requires the same observable behaviour:
/// success, return or revert data, logs, the ordered calls, creations and
/// value transfers, and the net storage changes the state-diff recorder sees. Gas is
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
        _assertSameNetWrites(a.accesses, b.accesses);
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

    /// Net storage changes: each slot written in a frame that did not
    /// revert, with the value it ends with, when that differs from its value
    /// before the call. The order of writes and writes that restore a value
    /// are not observable after the call.
    function _netWrites(Vm.AccountAccess[] memory all)
        internal
        pure
        returns (Vm.StorageAccess[] memory out)
    {
        uint256 total;
        for (uint256 i; i < all.length; ++i) {
            total += all[i].storageAccesses.length;
        }
        out = new Vm.StorageAccess[](total);
        uint256 n;
        for (uint256 i; i < all.length; ++i) {
            Vm.StorageAccess[] memory acc = all[i].storageAccesses;
            for (uint256 k; k < acc.length; ++k) {
                if (!acc[k].isWrite || acc[k].reverted) continue;
                bool found;
                for (uint256 m; m < n; ++m) {
                    if (out[m].account == acc[k].account && out[m].slot == acc[k].slot) {
                        out[m].newValue = acc[k].newValue;
                        found = true;
                        break;
                    }
                }
                if (!found) out[n++] = acc[k];
            }
        }
        uint256 kept;
        for (uint256 m; m < n; ++m) {
            if (out[m].newValue != out[m].previousValue) out[kept++] = out[m];
        }
        assembly ("memory-safe") {
            mstore(out, kept)
        }
    }

    function _assertSameNetWrites(
        Vm.AccountAccess[] memory a,
        Vm.AccountAccess[] memory b
    ) internal pure {
        Vm.StorageAccess[] memory wa = _netWrites(a);
        Vm.StorageAccess[] memory wb = _netWrites(b);
        assertEq(wa.length, wb.length, "changed slot count");
        for (uint256 i; i < wa.length; ++i) {
            bool found;
            for (uint256 j; j < wb.length; ++j) {
                if (wa[i].account == wb[j].account && wa[i].slot == wb[j].slot) {
                    assertEq(wa[i].newValue, wb[j].newValue, "slot value");
                    found = true;
                    break;
                }
            }
            assertTrue(found, "changed slot");
        }
    }
}
