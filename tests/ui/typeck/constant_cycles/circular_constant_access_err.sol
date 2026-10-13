// ported-from: test/libsolidity/syntaxTests/inlineAssembly/circular_constant_access_err.sol
contract C {
    bytes32 constant x = x;
    function f() public pure returns (uint t) {
        assembly {
            // Reference to a circular member
            t := x //~ ERROR: constant variable is circular
        }
    }
}
