contract C {
    uint256 value;

    function f() public {
        uint32 value; //~ WARN: this declaration shadows an existing declaration
        value = 2;
    }

    function g(uint256 value) public {
        uint32 value;
        value = 2;
    }
}
