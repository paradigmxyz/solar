//@ revisions: built opt
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[opt] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[opt] filecheck: --check-prefix=OPT

// BUILT-LABEL: fn @constructor(
// BUILT: sstore 1, [[SET_FLAG:[0-9]+]]
// OPT: @phase evm-shaped
contract FunctionPointerSignatures {
    bool flag;
    function() internal stateFn = setFlag;

    // BUILT-LABEL: fn @callVoid(
    // BUILT: internal_call @__internal_dispatch_0, 0, [[SET_FLAG]]
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[SET_FLAG]]
    // BUILT: internal_call @setFlag._1, 0
    // BUILT: mstore 4, 81
    // OPT-LABEL: fn @callVoid(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: internal_call @setFlag._1, 0
    // OPT: returndata 128, 32
    // OPT-LABEL: fn @__internal_dispatch_0(
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: tail_call @[[INVALID:__revert_stub[0-9]+]]
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
    // OPT-LABEL: fn @callState(
    // OPT: [[STORED:v[0-9]+]] = sload 1
    // OPT: internal_call @__internal_dispatch_0, 0, [[STORED]]
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
    // OPT-LABEL: fn @callPair(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: add arg0, 1
    // OPT: mstore 128, arg0
    // OPT: mstore 160, {{v[0-9]+}}
    // OPT: returndata 128, 64
    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    // BUILT-LABEL: fn @callZero(
    // BUILT: internal_call @__internal_dispatch_0, 0, 0
    // OPT-LABEL: fn @callZero(
    // OPT-NEXT: bb0:
    // OPT-NEXT: tail_call @[[INVALID]]
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
    // OPT-LABEL: fn @callTwoArgs(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: mstore 128, 6
    function callTwoArgs() public returns (uint256) {
        function(uint256, uint256) internal returns (uint256) sumFn = sum;
        return sumFn(5, 1);
    }
}
