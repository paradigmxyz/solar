// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/428_bare_revert.sol
contract C {
    function f(uint x) pure public {
        if (x > 7)
            revert; //~ ERROR: no matching declarations found
    }
}
