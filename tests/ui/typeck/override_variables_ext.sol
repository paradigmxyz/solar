// Additional tests for public variable override edge cases
// (error codes: 5225, 9098, 4822)

contract BaseWithPublicFunc {
    function getValue() public virtual returns (uint) { return 1; }
}

contract BaseWithExternalFunc {
    function getValue() external virtual returns (uint) { return 2; }
}

contract BaseWithExternalFuncWrongReturn {
    function getValue() external virtual returns (BaseWithExternalFuncWrongReturn) {}
}

// ERROR 5225: variable overriding non-external function
contract Bad1 is BaseWithPublicFunc {
    uint public override getValue;
    //~^ ERROR: public state variables can only override functions with external visibility
}

// OK: variable overriding external function with matching return type
contract Good1 is BaseWithExternalFunc {
    uint public override getValue;
}

// ERROR 4822: variable overriding function with different return type
contract Bad2 is BaseWithExternalFuncWrongReturn {
    uint public override getValue;
    //~^ ERROR: overriding public state variable return types differ
}
