//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck:

contract InternalCallFallbacks {
    // CHECK-LABEL: fn @recurse{{[( ]}}
    // CHECK: {{v[0-9]+}} = internal_call @a, 1, arg0
    function recurse(uint256 x) public returns (uint256) {
        return a(x);
    }

    // CHECK-LABEL: fn @a{{[( ]}}
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: internal_call @b, 1, [[NEXT]]
    function a(uint256 x) internal returns (uint256) {
        return x == 0 ? 0 : b(x - 1);
    }

    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: internal_call @a, 1, [[NEXT]]
    function b(uint256 x) internal returns (uint256) {
        return x == 0 ? 0 : a(x - 1);
    }

    // CHECK-LABEL: fn @multi{{[( ]}}
    // CHECK: internal_call @pair, 2, arg0
    // CHECK: mload 32
    // CHECK: returndata 128, 64
    function multi(uint256 x) public pure returns (uint256, uint256) {
        return pair(x);
    }

    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: [[SECOND:v[0-9]+]] = add arg0, 1
    // CHECK: ret arg0, [[SECOND]]
    function pair(uint256 x) internal pure returns (uint256, uint256) {
        return (x, x + 1);
    }
}
