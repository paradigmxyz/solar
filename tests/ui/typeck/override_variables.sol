// Tests for public state variable override checker

contract BaseWithFunc {
    function getValue() external virtual returns (uint) { return 1; }
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

// OK: public variable overriding external function
contract GoodVar1 is BaseWithFunc {
    uint public override getValue;
}
