// Tests for override specifier validation during resolution

contract A {
    function foo() public virtual returns (uint) { return 1; }
}

contract B {
    function bar() public virtual returns (uint) { return 2; }
}

// Error 1750: Free functions cannot override
function freeFunc() override(A) returns (uint) { return 42; }
//~^ ERROR: free functions cannot override

// Error 2353: Invalid contract in override list (not a base)
contract C is A {
    // B is not a base of C
    function foo() public override(A, B) returns (uint) { return 3; }
    //~^ ERROR: invalid contract `B` specified in override list
}

// Valid: proper override specifier
contract D is A {
    function foo() public override(A) returns (uint) { return 4; }
}
