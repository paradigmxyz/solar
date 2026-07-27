//@ revisions: built opt
//@[built] compile-flags: -Zcodegen -Zdump=mir
//@[built] filecheck: --check-prefix=BUILT
//@[opt] compile-flags: -Zcodegen -Ogas -Zdump=mir-evm-shaped
//@[opt] filecheck: --check-prefix=OPT

// ported-from: test/libsolidity/semanticTests/functionCall/call_function_returning_function.sol
// ported-from: test/libsolidity/semanticTests/array/function_memory_array.sol
// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

// BUILT-LABEL: fn @_anonymous(
// BUILT: sstore 1, [[SET_FLAG:[0-9]+]]
// BUILT: sstore 2, [[ONLY_STORED:[0-9]+]]
// OPT: @phase evm-shaped
contract InternalFunctionPointerMir {
    bool flag;
    function() internal stateFn = setFlag;
    function() internal returns (uint256) storedOnly;

    constructor() {
        storedOnly = onlyStored;
    }

    // BUILT-LABEL: fn @dynamic(
    // BUILT: mstore 0, [[INCREMENT:[0-9]+]]
    // BUILT: mstore 0, [[DECREMENT:[0-9]+]]
    // BUILT: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}, arg1
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[INCREMENT]]
    // BUILT: internal_call @increment, 1, arg1
    // BUILT: eq arg0, [[DECREMENT]]
    // BUILT: internal_call @decrement, 1, arg1
    // BUILT: eq arg0, [[INCREMENT_VIEW:[0-9]+]]
    // BUILT: internal_call @incrementView, 1, arg1
    // BUILT: mstore 4, 81
    // BUILT: revert 0, 36
    // OPT-LABEL: fn @dynamic(
    // OPT: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}, arg1
    // OPT-LABEL: fn @__internal_dispatch_0(
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: tail_call @[[INVALID:__revert_stub[0-9]+]]
    function dynamic(bool add, uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = add ? increment : decrement;
        return fn(value);
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function decrement(uint256 value) internal pure returns (uint256) {
        return value - 1;
    }

    // BUILT-LABEL: fn @callConstant(
    // BUILT: internal_call @__internal_dispatch_0, 1, [[INCREMENT]], arg0
    // OPT-LABEL: fn @callConstant(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: add arg0, 1
    // OPT: returndata 128, 32
    function callConstant(uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = increment;
        return fn(value);
    }

    // BUILT-LABEL: fn @callCast(
    // BUILT: [[CASTED:v[0-9]+]] = internal_call @castViewToPure, 1, [[INCREMENT_VIEW]]
    // BUILT: internal_call @__internal_dispatch_0, 1, [[CASTED]], arg0
    // BUILT-LABEL: fn @castViewToPure(
    // BUILT: mstore {{v[0-9]+}}, arg0
    // BUILT: ret {{v[0-9]+}}
    // OPT-LABEL: fn @callCast(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: number
    // OPT: add arg0, 1
    function callCast(uint256 value) public pure returns (uint256) {
        return castViewToPure(incrementView)(value);
    }

    function castViewToPure(
        function(uint256) internal view returns (uint256) fnIn
    ) internal pure returns (function(uint256) internal pure returns (uint256) fnOut) {
        assembly {
            fnOut := fnIn
        }
    }

    function incrementView(uint256 value) internal view returns (uint256) {
        if (block.number == type(uint256).max) return value;
        return value + 1;
    }

    // BUILT-LABEL: fn @callVoid(
    // BUILT: internal_call @__internal_dispatch_1, 0, [[SET_FLAG]]
    // BUILT-LABEL: fn @__internal_dispatch_1(
    // BUILT: eq arg0, [[SET_FLAG]]
    // BUILT: internal_call fn{{[0-9]+}}, 0
    // BUILT: mstore 4, 81
    // OPT-LABEL: fn @callVoid(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: internal_call fn{{[0-9]+}}, 0
    // OPT: returndata 128, 32
    // OPT-LABEL: fn @__internal_dispatch_1(
    // OPT: eq arg0, {{[0-9]+}}
    // OPT: tail_call @[[INVALID]]
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
    // BUILT: internal_call @__internal_dispatch_1, 0, [[STORED]]
    // OPT-LABEL: fn @callState(
    // OPT: [[STORED:v[0-9]+]] = sload 1
    // OPT: internal_call @__internal_dispatch_1, 0, [[STORED]]
    function callState() public returns (bool) {
        stateFn();
        return flag;
    }

    // BUILT-LABEL: fn @callPair(
    // BUILT: internal_call @__internal_dispatch_2, 2, [[PAIR:[0-9]+]], arg0
    // BUILT-LABEL: fn @__internal_dispatch_2(
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
    // BUILT: internal_call @__internal_dispatch_1, 0, 0
    // OPT-LABEL: fn @callZero(
    // OPT-NEXT: bb0:
    // OPT-NEXT: tail_call @[[INVALID]]
    function callZero() public {
        function() internal fn;
        fn();
    }

    // BUILT-LABEL: fn @higher0(
    // BUILT: returndata 128, 32
    // OPT-LABEL: fn @higher0(
    // OPT: mstore 128, 2
    function higher0() public pure returns (uint256) {
        return 2;
    }

    // BUILT-LABEL: fn @higher1(
    // BUILT: ret [[HIGHER0:[0-9]+]]
    function higher1() internal pure returns (function() internal returns (uint256)) {
        return higher0;
    }

    // BUILT-LABEL: fn @higher2(
    // BUILT: ret [[HIGHER1:[0-9]+]]
    function higher2()
        internal
        pure
        returns (function() internal returns (function() internal returns (uint256)))
    {
        return higher1;
    }

    // BUILT-LABEL: fn @higher3(
    // BUILT: ret [[HIGHER2:[0-9]+]]
    function higher3()
        internal
        pure
        returns (
            function()
                internal
                returns (function() internal returns (function() internal returns (uint256)))
        )
    {
        return higher2;
    }

    // BUILT-LABEL: fn @callReturned(
    // BUILT: internal_call @__internal_dispatch_3, 1, [[HIGHER3:[0-9]+]]
    // BUILT: internal_call @__internal_dispatch_3, 1, {{v[0-9]+}}
    // BUILT: internal_call @__internal_dispatch_3, 1, {{v[0-9]+}}
    // BUILT: internal_call @__internal_dispatch_4, 1, {{v[0-9]+}}
    // BUILT-LABEL: fn @__internal_dispatch_3(
    // BUILT: eq arg0, [[HIGHER1]]
    // BUILT: internal_call @higher1, 1
    // BUILT: eq arg0, [[HIGHER2]]
    // BUILT: internal_call @higher2, 1
    // BUILT: eq arg0, [[HIGHER3]]
    // BUILT: internal_call @higher3, 1
    // BUILT-LABEL: fn @__internal_dispatch_4(
    // BUILT: eq arg0, [[HIGHER0]]
    // BUILT: internal_call fn{{[0-9]+}}, 1
    // BUILT: eq arg0, [[ONLY_STORED]]
    // BUILT: internal_call @onlyStored, 1
    // OPT-LABEL: fn @callReturned(
    // OPT: internal_call @__internal_dispatch_3, 1, {{[0-9]+}}
    // OPT: internal_call @__internal_dispatch_4, 1, {{v[0-9]+}}
    // OPT-LABEL: fn @__internal_dispatch_3(
    // OPT: ret {{[0-9]+}}
    // OPT: ret {{[0-9]+}}
    // OPT: ret {{[0-9]+}}
    // OPT-LABEL: fn @__internal_dispatch_4(
    // OPT: ret 7
    function callReturned() public returns (uint256) {
        function()
            internal
            returns (
                function()
                    internal
                    returns (
                        function() internal returns (function() internal returns (uint256))
                    )
            ) fn = higher3;
        return fn()()()();
    }

    function sum(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    // BUILT-LABEL: fn @callTwoArgs(
    // BUILT: internal_call @__internal_dispatch_5, 1, [[SUM:[0-9]+]], 5, 1
    // BUILT-LABEL: fn @__internal_dispatch_5(
    // BUILT: eq arg0, [[SUM]]
    // BUILT: internal_call @sum, 1, arg1, arg2
    // OPT-LABEL: fn @callTwoArgs(
    // OPT-NOT: internal_call @__internal_dispatch
    // OPT: mstore 128, 6
    function callTwoArgs() public returns (uint256) {
        function(uint256, uint256) internal returns (uint256) sumFn = sum;
        return sumFn(5, 1);
    }

    function arrayA(uint256 x) public pure returns (uint256) {
        return x + 1;
    }

    function arrayB(uint256 x) public pure returns (uint256) {
        return x + 2;
    }

    function arrayC(uint256 x) public pure returns (uint256) {
        return x + 3;
    }

    function arrayD(uint256 x) public pure returns (uint256) {
        return x + 5;
    }

    function arrayE(uint256 x) public pure returns (uint256) {
        return x + 8;
    }

    // BUILT-LABEL: fn @callArray(
    // BUILT: mstore {{v[0-9]+}}, {{[0-9]+}}
    // BUILT: mstore {{v[0-9]+}}, {{[0-9]+}}
    // BUILT: mstore {{v[0-9]+}}, {{[0-9]+}}
    // BUILT: mstore {{v[0-9]+}}, {{[0-9]+}}
    // BUILT: mstore {{v[0-9]+}}, {{[0-9]+}}
    // BUILT: [[ARRAY_FN:v[0-9]+]] = mload {{v[0-9]+}}
    // BUILT: internal_call @__internal_dispatch_0, 1, [[ARRAY_FN]], arg0
    // OPT-LABEL: fn @callArray(
    // OPT: [[ARRAY_FN:v[0-9]+]] = mload {{v[0-9]+}}
    // OPT: internal_call @__internal_dispatch_0, 1, [[ARRAY_FN]], arg0
    function callArray(uint256 x, uint256 index) public returns (uint256) {
        function(uint256) internal returns (uint256)[] memory functions =
            new function(uint256) internal returns (uint256)[](10);
        functions[0] = arrayA;
        functions[1] = arrayB;
        functions[2] = arrayC;
        functions[3] = arrayD;
        functions[4] = arrayE;
        return functions[index](x);
    }

    function onlyStored() internal pure returns (uint256) {
        return 7;
    }

    // BUILT-LABEL: fn @callStoredOnly(
    // BUILT: [[STORED_ONLY:v[0-9]+]] = sload 2
    // BUILT: internal_call @__internal_dispatch_4, 1, [[STORED_ONLY]]
    // OPT-LABEL: fn @callStoredOnly(
    // OPT: [[STORED_ONLY:v[0-9]+]] = sload 2
    // OPT: internal_call @__internal_dispatch_4, 1, [[STORED_ONLY]]
    // OPT: fn @[[INVALID]](
    // OPT: mstore 4, 81
    // OPT: fn @__revert_stub{{[0-9]+}}(
    // OPT: mstore 4, 17
    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
