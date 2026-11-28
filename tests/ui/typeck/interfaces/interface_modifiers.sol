// ported-from: test/libsolidity/syntaxTests/modifiers/definition_in_interface.sol

interface I {
    modifier m { _; } //~ ERROR: modifiers cannot be defined in interfaces
}

// A modifier in an interface reports the modifier error and no function one: not the implicitly
// `virtual` warning, and not the visibility.
interface J {
    modifier m() { _; } //~ ERROR: modifiers cannot be defined in interfaces
    function f() external;
}

// The restriction only applies to interfaces.
contract C {
    modifier m() { _; }
    function f() public m {}
}
