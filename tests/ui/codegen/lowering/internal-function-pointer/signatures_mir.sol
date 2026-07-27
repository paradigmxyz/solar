//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck: --check-prefix=BUILT

// BUILT-LABEL: fn @constructor(
// BUILT: sstore 1, [[SET_FLAG:[0-9]+]]
contract FunctionPointerSignatures {
    bool flag;
    function() internal stateFn = setFlag;

    // BUILT-LABEL: fn @callVoid(
    // BUILT: internal_call @__internal_dispatch_0, 0, [[SET_FLAG]]
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[SET_FLAG]]
    // BUILT: internal_call setFlag{{[0-9]+}}, 0
    // BUILT: mstore 4, 81
    function callVoid() public returns (bool) {
        function() internal fn = setFlag;
        fn();
        return flag;
    }

    function setFlag() public {
        flag = true;
    }

    // BUILT-LABEL: fn @callState(
    // BUILT: [[STORED:v[0-9]+]] = sload 1
    // BUILT: internal_call @__internal_dispatch_0, 0, [[STORED]]
    function callState() public returns (bool) {
        stateFn();
        return flag;
    }

    // BUILT-LABEL: fn @callPair(
    // BUILT: internal_call @__internal_dispatch_1, 2, [[PAIR:[0-9]+]], arg0
    // BUILT-LABEL: fn @__internal_dispatch_1(
    // BUILT: eq arg0, [[PAIR]]
    // BUILT: [[FIRST:v[0-9]+]] = internal_call @pair, 2, arg1
    // BUILT: [[SECOND:v[0-9]+]] = mload {{v[0-9]+}}
    // BUILT: ret [[FIRST]], [[SECOND]]
    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    // BUILT-LABEL: fn @callZero(
    // BUILT: internal_call @__internal_dispatch_0, 0, 0
    function callZero() public {
        function() internal fn;
        fn();
    }

    function sum(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    // BUILT-LABEL: fn @callTwoArgs(
    // BUILT: internal_call @__internal_dispatch_2, 1, [[SUM:[0-9]+]], 5, 1
    // BUILT-LABEL: fn @__internal_dispatch_2(
    // BUILT: eq arg0, [[SUM]]
    // BUILT: internal_call @sum, 1, arg1, arg2
    function callTwoArgs() public returns (uint256) {
        function(uint256, uint256) internal returns (uint256) sumFn = sum;
        return sumFn(5, 1);
    }
}
