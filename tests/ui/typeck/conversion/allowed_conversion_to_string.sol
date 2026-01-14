//@compile-flags: -Ztypeck
// Solar rejects this explicit conversion (storage -> memory)
contract C {
    string a;
    string b;
    function f() public view {
        string storage c = a;
        string memory d = b;
        d = string(c); //~ ERROR: invalid explicit type conversion
    }
}
