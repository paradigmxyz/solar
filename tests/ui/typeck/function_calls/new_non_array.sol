//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/265_new_for_non_array.sol

contract C {
    function f() public {
        uint256 x = new uint256(7); //~ ERROR: expected contract or dynamic array type
    }
}
