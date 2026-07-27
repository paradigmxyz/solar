//@ run-call: choose true, 7 => 8
//@ run-call: choose false, 7 => 6
//@ run-call-fail: choose true, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: choose false, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: throughCast 9 => 10
//@ run-call-fail: throughCast 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: callVoid => true
//@ run-call: callPair 7 => 7, 8
//@ run-call: callState => true
//@ run-call-fail: callZero => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051
//@ run-call: callReturned => 2
//@ run-call: callTwoArgs => 6
//@ run-call: callArray 10, 0 => 11
//@ run-call: callArray 10, 1 => 12
//@ run-call: callArray 10, 2 => 13
//@ run-call: callArray 10, 3 => 15
//@ run-call: callArray 10, 4 => 18
//@ run-call-fail: callArray 10, 5 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051
//@ run-call: callStoredOnly => 7
//@ run-call: PointerDerived::callQualified => 1
//@ run-call: PointerDerived::callVirtual => 2

// ported-from: test/libsolidity/semanticTests/functionCall/call_function_returning_function.sol
// ported-from: test/libsolidity/semanticTests/array/function_memory_array.sol
// ported-from: test/libsolidity/semanticTests/inheritance/inherited_function_through_dispatch.sol
// ported-from: test/libsolidity/semanticTests/virtualFunctions/internal_virtual_function_calls_through_dispatch.sol
// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

contract InternalFunctionPointerCall {
    bool flag;
    function() internal stateFn = setFlag;
    function() internal returns (uint256) storedOnly;

    constructor() {
        storedOnly = onlyStored;
    }

    function choose(bool add, uint256 value) public returns (uint256) {
        function(uint256) internal returns (uint256) fn = add ? increment : decrement;
        return fn(value);
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function decrement(uint256 value) internal pure returns (uint256) {
        return value - 1;
    }

    function throughCast(uint256 value) public pure returns (uint256) {
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

    function callVoid() public returns (bool) {
        function() internal fn = setFlag;
        fn();
        return flag;
    }

    function setFlag() public {
        flag = true;
    }

    function callState() public returns (bool) {
        stateFn();
        return flag;
    }

    function callPair(uint256 value) public returns (uint256, uint256) {
        function(uint256) internal returns (uint256, uint256) fn = pair;
        return fn(value);
    }

    function pair(uint256 value) internal pure returns (uint256, uint256) {
        return (value, value + 1);
    }

    function callZero() public {
        function() internal fn;
        fn();
    }

    function higher0() public pure returns (uint256) {
        return 2;
    }

    function higher1() internal pure returns (function() internal returns (uint256)) {
        return higher0;
    }

    function higher2()
        internal
        pure
        returns (function() internal returns (function() internal returns (uint256)))
    {
        return higher1;
    }

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

    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}

contract PointerBase {
    function target() internal virtual returns (uint256) {
        return 1;
    }

    function callThroughVirtualPointer() internal returns (uint256) {
        function() internal returns (uint256) fn = target;
        return fn();
    }
}

contract PointerDerived is PointerBase {
    function target() internal pure override returns (uint256) {
        return 2;
    }

    function callQualified() public returns (uint256) {
        function() internal returns (uint256) fn = PointerBase.target;
        return fn();
    }

    function callVirtual() public returns (uint256) {
        return callThroughVirtualPointer();
    }
}
