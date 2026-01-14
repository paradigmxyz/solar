// Additional tests for override checker edge cases
// (error codes: 4520, 4593, 2353)

contract Base {
    function mustOverride() public virtual returns (uint) { return 1; }
    function implemented() public virtual returns (uint) { return 2; }
}

contract Base2 {
    function mustOverride() public virtual returns (uint) { return 10; }
}

contract Base3 {
    function other() public virtual {}
}

// ERROR 4520: duplicate contract in override list
contract Bad1 is Base, Base2 {
    function mustOverride() public override(Base, Base2, Base) returns (uint) { return 900; }
    //~^ ERROR: duplicate contract found in override list
}

// ERROR 4593: overriding implemented with unimplemented
abstract contract Bad2 is Base {
    function implemented() public override virtual returns (uint);
    //~^ ERROR: overriding an implemented function with an unimplemented function
}

// ERROR 2353: invalid contract in override list
// Note: The 2353 check is implemented, but error is caught earlier by resolver.
// Testing with a base that doesn't define the function being overridden:
contract Bad3 is Base, Base3 {
    function mustOverride() public override(Base, Base3) returns (uint) { return 800; }
    //~^ ERROR: invalid contract specified in override list
}

// OK: proper multi-inheritance override
contract Good1 is Base, Base2 {
    function mustOverride() public override(Base, Base2) returns (uint) { return 1000; }
}
