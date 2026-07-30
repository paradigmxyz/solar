//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck:

// CHECK-LABEL: fn @constructor(
// CHECK: sstore 1, [[SET_FLAG:[0-9]+]]
contract FunctionPointerSignatures {
    bool flag;
    function() internal stateFn = setFlag;

    // CHECK-LABEL: fn @callVoid(
    // CHECK: internal_call @__internal_dispatch_0, 0, [[SET_FLAG]]
    // CHECK-LABEL: fn @__internal_dispatch_0(
    // CHECK: eq arg0, [[SET_FLAG]]
    // CHECK: internal_call @setFlag.12, 0
    // CHECK: mstore 4, 81
    function callVoid() public returns (bool) {
        function() internal fn = setFlag;
        fn();
        return flag;
    }

    function setFlag() public {
        flag = true;
    }

    // CHECK-LABEL: fn @callState(
    // CHECK: [[STORED:v[0-9]+]] = sload 1
    // CHECK: internal_call @__internal_dispatch_0, 0, [[STORED]]
    function callState() public returns (bool) {
        stateFn();
        return flag;
    }

    // CHECK-LABEL: fn @callPair(
    // CHECK: internal_call @__internal_dispatch_1, 2, [[PAIR:[0-9]+]], arg0
    // CHECK-LABEL: fn @__internal_dispatch_1(
    // CHECK: eq arg0, [[PAIR]]
    // CHECK: [[FIRST:v[0-9]+]] = internal_call @pair, 2, arg1
    // CHECK: [[SECOND:v[0-9]+]] = mload {{v[0-9]+}}
    // CHECK: ret [[FIRST]], [[SECOND]]
    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    // CHECK-LABEL: fn @callZero(
    // CHECK: internal_call @__internal_dispatch_0, 0, 0
    function callZero() public {
        function() internal fn;
        fn();
    }

    function sum(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    // CHECK-LABEL: fn @callTwoArgs(
    // CHECK: internal_call @__internal_dispatch_2, 1, [[SUM:[0-9]+]], 5, 1
    // CHECK-LABEL: fn @__internal_dispatch_2(
    // CHECK: eq arg0, [[SUM]]
    // CHECK: internal_call @sum, 1, arg1, arg2
    function callTwoArgs() public returns (uint256) {
        function(uint256, uint256) internal returns (uint256) sumFn = sum;
        return sumFn(5, 1);
    }
}
