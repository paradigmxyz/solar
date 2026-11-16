// A cyclic inheritance graph stops at the base ordering error, even with override lists.

contract A1 {
    function f() external virtual {}
}
contract C1 is A1, B1 { //~ ERROR: definition of base has to precede definition of derived contract
    function f() external override(A1, B1) {}
}
contract B1 is C1 {}

// Three-way cycle.
contract A2 is C2 { //~ ERROR: definition of base has to precede definition of derived contract
    function f() public virtual {}
}
contract B2 is A2 {
    function f() public virtual override(A2, C2) {}
}
contract C2 is B2 {
    function f() public virtual override(A2, B2) {}
}

// Three-way cycle with a common base.
contract X3 {
    function f() public virtual {}
}
contract A3 is X3, C3 { //~ ERROR: definition of base has to precede definition of derived contract
    function f() public virtual override(X3, C3) {}
}
contract B3 is A3 {
    function f() public virtual override(A3) {}
}
contract C3 is B3 {
    function f() public virtual override(B3, X3) {}
}

// Contracts that inherit from a cycle are not checked again.
contract D3 is A3 {
    function f() public override(A3, B3) {}
}
contract E3 is D3, B3 {
    function f() public override(D3, B3) {}
}

// Modifiers and public state variables.
contract A4 is B4 { //~ ERROR: definition of base has to precede definition of derived contract
    modifier m() virtual {
        _;
    }
}
contract B4 is A4 {
    modifier m() override {
        _;
    }
}

contract A5 is B5 { //~ ERROR: definition of base has to precede definition of derived contract
    uint public x;
}
contract B5 is A5 {
    function x() external override(A5) returns (uint) {}
}
