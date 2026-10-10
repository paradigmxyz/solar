// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/045_returning_multi_dimensional_arrays.sol
pragma abicoder v1;
contract C {
    function f() public pure returns (string[][] memory) {} //~ ERROR: this type is only supported in ABI coder v2
}
