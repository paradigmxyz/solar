// ported-from: test/libsolidity/syntaxTests/modifiers/definition_in_interface.sol

interface I {
    modifier m { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    modifier mu; //~ ERROR: modifiers cannot be defined or declared in interfaces
    //~^ ERROR: modifiers without implementation must be marked `virtual`
    modifier mv virtual { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    modifier muv virtual; //~ ERROR: modifiers cannot be defined or declared in interfaces
}

// A modifier in an interface reports the modifier errors and no function one: not the implicitly
// `virtual` warning, and not the visibility. `virtual` is only implied for functions, so an
// unimplemented modifier still has to be marked `virtual`.
interface J {
    modifier m() virtual { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    function f() external;
}

// The restriction only applies to interfaces.
contract C {
    modifier m() { _; }
    function f() public m {}
}
