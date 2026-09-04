// ported-from: test/libsolidity/syntaxTests/freeFunctions/free_virtual.sol

function fun() virtual {} //~ ERROR: free functions cannot be `virtual`

// The restriction only applies to free functions.
function fun2() {}

contract C {
    function f() public virtual {}
}

interface I {
    function f() external virtual;
}

library L {
    function f() internal pure returns (uint) { return 0; }
}
