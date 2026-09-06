//@ revisions: mir runtime
//@[mir] compile-flags: -O none -Zdump=mir
//@[mir] filecheck:
//@ run-call: callEntry 41 => 42

contract ICallFallbacks {
    // CHECK-LABEL: fn @recurse{{[( ]}}
    // CHECK: {{v[0-9]+}} = icall @a, arg0
    function recurse(uint256 x) public returns (uint256) {
        return a(x);
    }

    // CHECK-LABEL: fn @a{{[( ]}}
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: icall @b, [[NEXT]]
    function a(uint256 x) internal returns (uint256) {
        return x == 0 ? 0 : b(x - 1);
    }

    // CHECK-LABEL: fn @b{{[( ]}}
    // CHECK: [[NEXT:v[0-9]+]] = sub arg0, 1
    // CHECK: icall @a, [[NEXT]]
    function b(uint256 x) internal returns (uint256) {
        return x == 0 ? 0 : a(x - 1);
    }

    // CHECK-LABEL: fn @multi{{[( ]}}
    // CHECK: [[PAIR:v[0-9]+]] = icall @pair, arg0
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 0
    // CHECK: extract_value {{struct[0-9]+}}, [[PAIR]], 1
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, {{v[0-9]+}}
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, {{v[0-9]+}}
    // CHECK: ret [[RET_1]]
    function multi(uint256 x) public pure returns (uint256, uint256) {
        return pair(x);
    }

    // CHECK-LABEL: fn @pair{{[( ]}}
    // CHECK: [[SECOND:v[0-9]+]] = add arg0, 1
    // CHECK: [[RET_0:v[0-9]+]] = insert_value [[RET_TY:struct[0-9]+]], undef [[RET_TY]], 0, arg0
    // CHECK: [[RET_1:v[0-9]+]] = insert_value [[RET_TY]], [[RET_0]], 1, [[SECOND]]
    // CHECK: ret [[RET_1]]
    function pair(uint256 x) internal pure returns (uint256, uint256) {
        return (x, x + 1);
    }

    // CHECK-LABEL: fn @callVoid{{[( ]}}
    // CHECK-NOT: = icall @branchingVoid, arg0
    // CHECK: icall @branchingVoid, arg0
    function callVoid(uint256 x) public pure {
        branchingVoid(x);
    }

    function branchingVoid(uint256 x) internal pure {
        if (x != 0) branchingVoid(x - 1);
    }

    function callEntry(uint256 x) public pure returns (uint256) {
        return entry(x);
    }

    function entry(uint256 x) internal pure returns (uint256 result) {
        for (uint256 i = 0; i < x; i++) {
            result++;
        }
        return result + 1;
    }
}
