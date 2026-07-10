//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/assign_attached_library_function.sol

library L {
    function foo(uint256 a, uint256 b) internal pure returns (uint256) {
        return a + b;
    }
}
contract C {
    using L for uint256;

    function bar() public {
        uint256 x;
        function(uint256, uint256) internal pure returns (uint256) ptr = x.foo;
        //~^ ERROR: mismatched types
        ptr;
    }
}
