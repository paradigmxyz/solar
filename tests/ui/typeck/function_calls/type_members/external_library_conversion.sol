//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/functionTypes/external_library_function_to_external_function_type.sol

library L {
    function f(uint256) external returns (uint256) {}
}
contract C {
    function run(function(uint256) external returns (uint256) operation) internal returns (uint256) {
        operation;
    }

    function test() public {
        run(L.f); //~ ERROR: mismatched types
        function(uint256) external returns (uint256) operation = L.f; //~ ERROR: mismatched types
        operation;
    }
}
