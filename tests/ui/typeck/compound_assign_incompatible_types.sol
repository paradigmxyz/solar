// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/compoundAssignment/incomp_types.sol

contract C {
    function f() public returns (uint a, uint b) {
        a += (1, 1); //~ ERROR: cannot apply builtin operator
    }
}
