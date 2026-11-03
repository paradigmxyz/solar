//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir-final
//@[dump] filecheck:
//@ run-call: helperReturn 1 => 0x10000000000000000000000000000010001
//@ run-call: directThenRevertHelper 2, false => 0x10000000000000000000000000000010001
//@ run-call: revertHelper 4, false => 0x10000000000000000000000000000010001
//@ run-call: split 5, false => 0x10000000000000000000000000000010001
//@ run-call: nestedReturn 6 => 0x10000000000000000000000000000010001
//@ run-call: recursiveReturn 7, 2 => 0x10000000000000000000000000000010001

// Which gas reads keep packed field writes apart when the caller reads gas
// again after them. A helper's read starts a measurement only if the helper can
// return after it; a read on a path that only reverts measures nothing the
// caller runs afterwards. A direct read in the caller always does.
contract GasObservationEdges {
    struct S {
        bool a;
        bool b;
        uint120 n;
        uint120 d;
    }

    mapping(uint256 => S) internal s;
    uint256 internal sink;

    // The helper can return after reading gas, so all four writes stay.
    // CHECK-LABEL: fn @helperReturn(
    // CHECK-COUNT-4: sstore
    // CHECK: = gas
    function helperReturn(uint256 h) external returns (uint256) {
        readThenReturn();
        S storage x = s[h];
        x.a = true;
        x.b = false;
        x.n = 1;
        x.d = 1;
        sink = gasleft();
        return raw(h);
    }

    // A direct read before the helper starts the measurement, so all four stay.
    // CHECK-LABEL: fn @directThenRevertHelper(
    // CHECK: = gas
    // CHECK-COUNT-4: sstore
    // CHECK: = gas
    function directThenRevertHelper(uint256 h, bool f) external returns (uint256) {
        uint256 g0 = gasleft();
        readThenRevert(f);
        S storage x = s[h];
        x.a = true;
        x.b = false;
        x.n = 1;
        x.d = 1;
        sink = g0 - gasleft();
        return raw(h);
    }

    // The helper reads gas only before reverting, so the writes merge.
    // CHECK-LABEL: fn @revertHelper(
    // CHECK: sstore {{.*}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore {{.*}}, {{.*}} !metadata(storage=symbolic
    // CHECK: = gas
    function revertHelper(uint256 h, bool f) external returns (uint256) {
        readThenRevert(f);
        S storage x = s[h];
        x.a = true;
        x.b = false;
        x.n = 1;
        x.d = 1;
        sink = gasleft();
        return raw(h);
    }

    // On the reverting path the helper's read follows the first writes, so
    // they stay; the writes after the helper merge.
    // CHECK-LABEL: fn @split(
    // CHECK-COUNT-2: sstore
    // CHECK: = gas
    function split(uint256 h, bool f) external returns (uint256) {
        S storage x = s[h];
        x.a = true;
        x.b = false;
        readThenRevert(f);
        x.n = 1;
        x.d = 1;
        sink = gasleft();
        return raw(h);
    }

    // The helper reaches a returning read through another call, so all four writes stay.
    // CHECK-LABEL: fn @nestedReturn(
    // CHECK-COUNT-4: sstore
    // CHECK: = gas
    function nestedReturn(uint256 h) external returns (uint256) {
        outer();
        S storage x = s[h];
        x.a = true;
        x.b = false;
        x.n = 1;
        x.d = 1;
        sink = gasleft();
        return raw(h);
    }

    // A read that a recursive helper reaches only through its own calls can still return, so
    // all four writes stay.
    // CHECK-LABEL: fn @recursiveReturn(
    // CHECK-COUNT-4: sstore
    // CHECK: = gas
    function recursiveReturn(uint256 h, uint256 depth) external returns (uint256) {
        recurse(depth);
        S storage x = s[h];
        x.a = true;
        x.b = false;
        x.n = 1;
        x.d = 1;
        sink = gasleft();
        return raw(h);
    }

    function raw(uint256 h) internal view returns (uint256 word) {
        S storage x = s[h];
        assembly {
            word := sload(x.slot)
        }
    }

    /// @custom:solar-inline never
    function readThenReturn() internal view {
        assembly {
            if lt(gas(), 10) { revert(0, 0) }
        }
    }

    /// @custom:solar-inline never
    function readThenRevert(bool f) internal view {
        if (f) {
            uint256 g = gasleft();
            assembly {
                mstore(0, g)
                revert(0, 32)
            }
        }
    }

    /// @custom:solar-inline never
    function outer() internal view {
        readThenReturn();
    }

    /// @custom:solar-inline never
    function recurse(uint256 depth) internal view {
        if (depth == 0) {
            readThenReturn();
        } else {
            recurse(depth - 1);
        }
    }
}
