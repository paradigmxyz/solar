// Tests for modifier override checker

contract BaseModifier {
    modifier onlyOwner() virtual { _; }
    modifier notVirtualMod() { _; }
    //~^ ERROR: trying to override non-virtual modifier
    modifier withParam(uint x) virtual { _; }
}

contract BaseModifier2 {
    modifier onlyOwner() virtual { _; }
}

// ERROR 9456: missing override on modifier
contract BadMod1 is BaseModifier {
    modifier onlyOwner() { _; }
    //~^ ERROR: overriding modifier is missing "override" specifier
}

// ERROR 4334: base modifier not virtual (error on line 5)
contract BadMod2 is BaseModifier {
    modifier notVirtualMod() override { _; }
}

// ERROR 4327: multi-inheritance modifier override
contract BadMod3 is BaseModifier, BaseModifier2 {
    modifier onlyOwner() override { _; }
    //~^ ERROR: Modifier needs to specify overridden contracts
}

// ERROR 7792: override without base modifier
contract BadMod4 {
    modifier noBaseMod() override { _; }
    //~^ ERROR: Modifier has override specified but does not override anything
}

// ERROR 6480: diamond inheritance - must override conflicting modifier
contract BadMod5 is BaseModifier, BaseModifier2 {}
//~^ ERROR: derived contract must override modifier "onlyOwner"

// ERROR 1078: modifier signature mismatch
contract BadMod6 is BaseModifier {
    modifier withParam(uint x, uint y) override { _; }
    //~^ ERROR: override changes modifier signature
}

// OK: proper modifier override
contract GoodMod1 is BaseModifier {
    modifier onlyOwner() override { _; }
}

// OK: multi-inheritance modifier with proper specifier
contract GoodMod2 is BaseModifier, BaseModifier2 {
    modifier onlyOwner() override(BaseModifier, BaseModifier2) { _; }
}
