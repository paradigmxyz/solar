// Modifiers cannot be overridden.

contract BaseModifier {
    modifier onlyOwner() { _; }
    modifier withParam(uint x) { _; }
}

contract BaseModifier2 {
    modifier onlyOwner() { _; }
}

// A derived contract cannot redefine an inherited modifier, whatever its parameters.
contract Redefines is BaseModifier {
    modifier onlyOwner() { _; } //~ ERROR: modifier `onlyOwner` is already defined in a base contract
    modifier withParam(uint x, uint y) { _; } //~ ERROR: modifier `withParam` is already defined in a base contract
}

// Two bases that define a modifier with the same name conflict, whatever their parameters.
contract Ambiguous is BaseModifier, BaseModifier2 {}
//~^ ERROR: two or more base classes define modifier `onlyOwner`

contract BaseModifier3 {
    modifier withParam() { _; }
}

contract AmbiguousParams is BaseModifier, BaseModifier3 {}
//~^ ERROR: two or more base classes define modifier `withParam`

// One definition inherited through two paths does not conflict.
contract Middle is BaseModifier {}

contract Diamond is BaseModifier, Middle {
    function f() public onlyOwner {}
}
