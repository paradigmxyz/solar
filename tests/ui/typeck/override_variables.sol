// Tests for public state variable override checker

contract BaseWithFunc {
    function getValue() external virtual returns (uint) { return 1; }
}

contract BaseWithModifier {
    modifier onlyOwner() virtual { _; }
}

contract BaseWithPublicVar {
    uint public myVar;
}

// ERROR 8022: override on non-public variable
contract BadVar1 {
    uint internal override x;
    //~^ ERROR: override can only be used with public state variables
}

// ERROR 7792: public variable override but nothing to override
contract BadVar2 {
    uint public override noBase;
    //~^ ERROR: Function has override specified but does not override anything
    //~| ERROR: public state variable has override specified but does not override anything
}

// ERROR 1456: public variable with same name as inherited modifier
// Note: This is caught by the resolver as a name conflict before we reach the override checker.
// The 1456 error in override_checker.rs handles the case where inherited_modifiers contains the name,
// but in practice, the resolver name collision check runs first.

// ERROR 1452: trying to override a public state variable
// Note: The 1452 error is emitted when check_override sees base.is_variable().
// This is caught by the resolver name conflict check first when names collide.

// OK: public variable overriding external function
contract GoodVar1 is BaseWithFunc {
    uint public override getValue;
}
