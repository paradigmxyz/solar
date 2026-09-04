//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// ported-from: test/libsolidity/semanticTests/functionCall/call_function_returning_function.sol

contract HigherOrderFunctionPointer {
    // CHECK-LABEL: fn @higher0(
    // CHECK: ret 2
    function higher0() public pure returns (uint256) {
        return 2;
    }

    // CHECK-LABEL: fn @higher1(
    // CHECK: ret [[HIGHER0:[0-9]+]]
    function higher1() internal pure returns (function() internal returns (uint256)) {
        return higher0;
    }

    // CHECK-LABEL: fn @higher2(
    // CHECK: ret [[HIGHER1:[0-9]+]]
    function higher2()
        internal
        pure
        returns (function() internal returns (function() internal returns (uint256)))
    {
        return higher1;
    }

    // CHECK-LABEL: fn @higher3(
    // CHECK: ret [[HIGHER2:[0-9]+]]
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

    // CHECK-LABEL: fn @callReturned(
    // CHECK: icall @[[DISPATCHER:internal_dispatcher_[A-Za-z0-9_]+]], 1, [[HIGHER3:[0-9]+]]
    // CHECK: icall @[[DISPATCHER]], 1, {{v[0-9]+}}
    // CHECK: icall @[[DISPATCHER]], 1, {{v[0-9]+}}
    // CHECK: icall @[[DISPATCHER_1:internal_dispatcher_[A-Za-z0-9_]+]], 1, {{v[0-9]+}}
    // CHECK: fn @[[DISPATCHER]](
    // CHECK: eq arg0, [[HIGHER1]]
    // CHECK: eq arg0, [[HIGHER2]]
    // CHECK: eq arg0, [[HIGHER3]]
    // CHECK: icall @higher3, 1
    // CHECK: icall @higher2, 1
    // CHECK: icall @higher1, 1
    // CHECK: fn @[[DISPATCHER_1]](
    // CHECK: eq arg0, [[HIGHER0]]
    // CHECK: icall @higher0, 1
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
