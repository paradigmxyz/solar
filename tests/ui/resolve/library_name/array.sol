// ported-from: test/libsolidity/syntaxTests/array/invalid/library_array.sol

library L {}
contract C {
    function f() public {
        L[] memory x; //~ ERROR: invalid use of a library name
    }
}
