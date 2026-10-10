// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/048_returning_arrays_in_structs_arrays.sol
pragma abicoder v1;
contract C {
    struct S { string[] s; }
    function f() public pure returns (S memory x) {} //~ ERROR: this type is only supported in ABI coder v2
}
