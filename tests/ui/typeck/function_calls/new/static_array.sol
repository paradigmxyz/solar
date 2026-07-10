//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/array/new_no_parentheses.sol

contract C {
    function f() public {
        new uint256[1]; //~ ERROR: length has to be placed in parentheses after the array type
    }
}
