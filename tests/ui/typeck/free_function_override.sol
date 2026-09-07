// ported-from: test/libsolidity/syntaxTests/freeFunctions/free_override.sol

function fun() override { //~ ERROR: free functions cannot override
}

// An override specifier with a contract list is rejected the same way.
function fun2() override(A) {} //~ ERROR: free functions cannot override

// The restriction only applies to free functions.
contract A {
    function foo() public virtual {}
}

contract B is A {
    function foo() public override {}
}
