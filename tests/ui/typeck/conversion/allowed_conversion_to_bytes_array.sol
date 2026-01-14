//@compile-flags: -Ztypeck
// Solar rejects this explicit conversion (storage -> memory)
contract C {
    bytes a;
    bytes b;
    function f() public view {
        bytes storage c = a;
        bytes memory d = b;
        d = bytes(c); //~ ERROR: invalid explicit type conversion
    }
}
