//@ filecheck:
// CHECK: @module
//@ revisions: none gas size mir
//@[none] compile-flags: -O none --emit=abi,bin
//@[gas] compile-flags: -O gas --emit=abi,bin
//@[size] compile-flags: -O size --emit=abi,bin
//@[mir] compile-flags: -O none -Zdump=mir
//@ run-call: test() => 12
// ported-from: test/libsolidity/semanticTests/functionCall/call_internal_function_with_multislot_arguments_via_pointer.sol

contract InternalFunctionPointerMultislot {
    function m(
        function() external returns (uint256) a,
        function() external returns (uint256) b
    ) internal pure returns (function() external returns (uint256)) {
        return a;
    }

    function s(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }

    function foo() external pure returns (uint256) {
        return 6;
    }

    function test() public returns (uint256) {
        function(uint256, uint256) internal returns (uint256) singleSlotFunction = s;
        function(
            function() external returns (uint256),
            function() external returns (uint256)
        ) internal returns (function() external returns (uint256)) multiSlotFunction = m;

        return multiSlotFunction(this.foo, this.foo)() + singleSlotFunction(5, 1);
    }
}
