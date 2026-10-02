//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Multi-value returns from internal *calls* must propagate all N values, not
// just the first. Previously the non-inlined `icall` carried a return
// count of 1 (so the backend never copied returns 2..N to scratch memory) and a
// bare `return lib.f()` returned only the first value. Runtime-verified against
// solc: `sat(5,3) == 8`, `tryA(7,3) == (true, 10)`.
library Math {
    function tryAdd(uint256 a, uint256 b) internal pure returns (bool ok, uint256 c) {
        unchecked {
            c = a + b;
            ok = c >= a;
        }
    }
}

contract C {
    // Destructuring a multi-value internal call: both `ok` and `c` must bind.
    // CHECK-LABEL: fn @sat{{[( ]}}
    // CHECK: [[PAIR:v[0-9]+]] = icall @tryAdd, arg0, arg1
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 0
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 1
    function sat(uint256 a, uint256 b) public pure returns (uint256) {
        (bool ok, uint256 c) = Math.tryAdd(a, b);
        if (!ok) return type(uint256).max;
        return c;
    }

    // `return lib.f()` must return both tuple values, not just the first.
    // CHECK-LABEL: fn @tryA{{[( ]}}
    // CHECK: [[PAIR:v[0-9]+]] = icall @tryAdd, arg0, arg1
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 0
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 1
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, {{v[0-9]+}}
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, {{v[0-9]+}}
    // CHECK: ret [[RET_1]]
    function tryA(uint256 a, uint256 b) public pure returns (bool, uint256) {
        return Math.tryAdd(a, b);
    }
}
