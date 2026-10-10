// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/046_returning_multi_dimensional_static_arrays.sol
pragma abicoder v1;
contract C {
    function f() public pure returns (uint[][2] memory) {} //~ ERROR: this type is only supported in ABI coder v2
}
