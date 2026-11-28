//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir-final
//@[dump] filecheck:
//@ run-call: helperReturn 1 => 0x10000000000000000000000000000010001
//@ run-call: split 5, false => 0x10000000000000000000000000000010001
//@ run-call-fail: split 5, true
//@ run-call: nestedReturn 6 => 0x10000000000000000000000000000010001
//@ run-call: recursiveReturn 7, 2 => 0x10000000000000000000000000000010001

// Gas reads, in the caller or in the helpers it calls, hold back no storage writes: the four
// packed field writes merge into one store of the whole word.
contract PackedWritesAcrossGas {
    struct S {
        bool a;
        bool b;
        uint120 n;
        uint120 d;
    }

    mapping(uint256 => S) internal s;
    uint256 internal sink;

    // CHECK-LABEL: fn @helperReturn(
    // CHECK-NOT: sstore
    // CHECK: sstore {{v[0-9]+}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore {{v[0-9]+}},
    // CHECK: sstore 1,
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

    // The writes before a helper that may revert merge with the ones after it: a revert discards
    // them anyway.
    // CHECK-LABEL: fn @split(
    // CHECK-NOT: sstore
    // CHECK: icall @readThenRevert
    // CHECK-NOT: sstore
    // CHECK: sstore {{v[0-9]+}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore {{v[0-9]+}},
    // CHECK: sstore 1,
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

    // CHECK-LABEL: fn @nestedReturn(
    // CHECK-NOT: sstore
    // CHECK: sstore {{v[0-9]+}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore {{v[0-9]+}},
    // CHECK: sstore 1,
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

    // CHECK-LABEL: fn @recursiveReturn(
    // CHECK-NOT: sstore
    // CHECK: sstore {{v[0-9]+}}, 0x10000000000000000000000000000010001
    // CHECK-NOT: sstore {{v[0-9]+}},
    // CHECK: sstore 1,
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
            require(gasleft() == 0);
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
