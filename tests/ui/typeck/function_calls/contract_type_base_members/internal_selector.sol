//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/484_function_types_selector_1.sol

contract C {
    function f() public view returns (bytes4) {
        return f.selector; //~ ERROR: member `selector` not found
    }
}
