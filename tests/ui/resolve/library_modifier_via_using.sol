// ported-from: test/libsolidity/syntaxTests/modifiers/library_via_using.sol

library L {
    modifier m() {
        _;
    }
}

contract C {
    using L for *;

    function f() L.m public {} //~ ERROR: can only use modifiers defined in the current contract or in base contracts
}
