//@ run-call: callReturned => 2

// ported-from: test/libsolidity/semanticTests/functionCall/call_function_returning_function.sol

contract HigherOrderFunctionPointer {
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
}
