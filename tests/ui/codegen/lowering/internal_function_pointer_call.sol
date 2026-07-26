//@ run-call: throughReturn => 42
//@ run-call: choose true, 7 => 8
//@ run-call: choose false, 7 => 6
//@ run-call: throughCast 9 => 10
//@ run-call: callVoid => true
//@ run-call-fail: callZero => 0x4e487b710000000000000000000000000000000000000000000000000000000000000051

// ported-from: test/libsolidity/semanticTests/functionCall/call_internal_function_via_expression.sol

contract InternalFunctionPointerCall {
    bool flag;

    function throughReturn() public returns (uint256) {
        return getPointer(answer)();
    }

    function getPointer(
        function() internal returns (uint256) fn
    ) internal pure returns (function() internal returns (uint256)) {
        return fn;
    }

    function answer() internal pure returns (uint256) {
        return 42;
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

    function setFlag() internal {
        flag = true;
    }

    function callZero() public {
        function() internal fn;
        fn();
    }
}
