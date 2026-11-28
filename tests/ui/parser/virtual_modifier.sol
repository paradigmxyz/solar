// Virtual modifiers have been removed, so modifiers cannot be `virtual`, `override`, or
// unimplemented.

abstract contract A {
    modifier a() virtual { _; } //~ ERROR: modifiers cannot be marked `virtual` or `override`
    modifier b() override { _; } //~ ERROR: modifiers cannot be marked `virtual` or `override`
    modifier c() override(A) { _; } //~ ERROR: modifiers cannot be marked `virtual` or `override`
    modifier d virtual override { _; }
    //~^ ERROR: modifiers cannot be marked `virtual` or `override`
    //~| ERROR: modifiers cannot be marked `virtual` or `override`
    modifier e(); //~ ERROR: modifiers without implementation are not allowed
    modifier f() virtual; //~ ERROR: modifiers cannot be marked `virtual` or `override`
    //~^ ERROR: modifiers without implementation are not allowed

    function g() public a {}
}

// An unimplemented modifier does not make the contract abstract.
contract B {
    modifier m(); //~ ERROR: modifiers without implementation are not allowed

    function g() public m {}
}
