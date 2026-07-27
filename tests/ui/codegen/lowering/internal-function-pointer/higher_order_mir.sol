//@ compile-flags: -Zcodegen -O none -Zdump=mir
//@ filecheck: --check-prefix=BUILT

// ported-from: test/libsolidity/semanticTests/functionCall/call_function_returning_function.sol

contract HigherOrderFunctionPointer {
    // BUILT-LABEL: fn @higher0(
    // BUILT: returndata 128, 32
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
    // BUILT: internal_call @__internal_dispatch_0, 1, [[HIGHER3:[0-9]+]]
    // BUILT: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}
    // BUILT: internal_call @__internal_dispatch_0, 1, {{v[0-9]+}}
    // BUILT: internal_call @__internal_dispatch_1, 1, {{v[0-9]+}}
    // BUILT-LABEL: fn @__internal_dispatch_0(
    // BUILT: eq arg0, [[HIGHER1]]
    // BUILT: internal_call @higher1, 1
    // BUILT: eq arg0, [[HIGHER2]]
    // BUILT: internal_call @higher2, 1
    // BUILT: eq arg0, [[HIGHER3]]
    // BUILT: internal_call @higher3, 1
    // BUILT-LABEL: fn @__internal_dispatch_1(
    // BUILT: eq arg0, [[HIGHER0]]
    // BUILT: internal_call higher0{{[0-9]+}}, 1
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
