// Additional tests for public variable override edge cases
// (error codes: 5225, 9098)

contract BaseWithPublicFunc {
    function getValue() public virtual returns (uint) { return 1; }
}

contract BaseWithExternalFunc {
    function getValue() external virtual returns (uint) { return 2; }
}

// ERROR 5225: variable overriding non-external function
// ERROR 9098: visibility mismatch (public vs external)
contract Bad1 is BaseWithPublicFunc {
    uint public override getValue;
    //~^ ERROR: overriding function visibility differs
    //~| ERROR: public state variables can only override functions with external visibility
}

// OK: variable overriding external function
contract Good1 is BaseWithExternalFunc {
    uint public override getValue;
}
