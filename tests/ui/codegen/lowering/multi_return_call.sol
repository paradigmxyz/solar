//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

// Multi-value returns from internal *calls* must propagate all N values, not
// just the first. Previously the non-inlined `internal_call` carried a return
// count of 1 (so the backend never copied returns 2..N to scratch memory) and a
// bare `return lib.f()` returned only the first value. Runtime-verified against
// solc: `sat(5,3) == 8`, `tryA(7,3) == (true, 10)`.
library Math {
    // CHECK-LABEL: fn @tryAdd
    // CHECK: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK: ret {{v[0-9]+}}, {{v[0-9]+}}
    function tryAdd(uint256 a, uint256 b) internal pure returns (bool ok, uint256 c) {
        unchecked {
            c = a + b;
            ok = c >= a;
        }
    }
}

contract C {
    // Destructuring a multi-value internal call: both `ok` and `c` must bind.
    // CHECK-LABEL: fn @sat
    // CHECK: internal_call @tryAdd, 2, arg0, arg1
    // CHECK: [[RETURNS:v[0-9]+]] = mload 32
    // CHECK: mload {{v[0-9]+}}
    function sat(uint256 a, uint256 b) public pure returns (uint256) {
        (bool ok, uint256 c) = Math.tryAdd(a, b);
        if (!ok) return type(uint256).max;
        return c;
    }

    // `return lib.f()` must return both tuple values, not just the first.
    // CHECK-LABEL: fn @tryA
    // CHECK: internal_call @tryAdd, 2, arg0, arg1
    // CHECK: [[RETURNS:v[0-9]+]] = mload 32
    // CHECK: mload {{v[0-9]+}}
    // CHECK: returndata 128, 64
    function tryA(uint256 a, uint256 b) public pure returns (bool, uint256) {
        return Math.tryAdd(a, b);
    }
}
