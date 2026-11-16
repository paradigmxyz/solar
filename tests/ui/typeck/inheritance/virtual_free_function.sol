// ported-from: test/libsolidity/syntaxTests/freeFunctions/free_virtual.sol

function fun() virtual {} //~ ERROR: free functions cannot be `virtual`

// An unimplemented free function is an error of its own, reported first.
function fun2() virtual; //~ ERROR: free functions must be implemented
//~^ ERROR: free functions cannot be `virtual`

// The restriction only applies to free functions.
function fun3() {}

contract C {
    function f() public virtual {}
}

interface I {
    function f() external virtual; //~ WARN: interface functions are implicitly `virtual`
}

library L {
    function f() internal pure returns (uint) { return 0; }
}
