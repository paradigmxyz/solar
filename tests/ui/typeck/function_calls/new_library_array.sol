//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/array/library_array.sol

library L {}

contract C {
    function f() public {
        new L[](2); //~ ERROR: invalid use of a library name
    }
}
