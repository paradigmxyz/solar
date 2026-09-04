// ported-from: test/libsolidity/syntaxTests/modifiers/definition_in_interface.sol

interface I {
    modifier m { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    modifier mu; //~ ERROR: modifiers cannot be defined or declared in interfaces
    modifier mv virtual { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    modifier muv virtual; //~ ERROR: modifiers cannot be defined or declared in interfaces
}

// A modifier in an interface reports this error and no function one: not the implicitly `virtual`
// warning, not the missing implementation, and not the visibility.
interface J {
    modifier m() virtual { _; } //~ ERROR: modifiers cannot be defined or declared in interfaces
    function f() external;
}

// The restriction only applies to interfaces.
contract C {
    modifier m() { _; }
    function f() public m {}
}
