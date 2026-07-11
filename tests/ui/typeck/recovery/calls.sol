contract C {
    function g(uint256 a, uint256 b) public {}

    function namedArgsRecover() public {
        this.g({a: 1,, b: 2}); //~ ERROR: expected identifier, found `,`
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function abiDecodeEmptyType(bytes memory data) public {
        uint256 x = abi.decode(data, (, uint256));
        //~^ ERROR: `abi.decode` type tuple components cannot be empty
        //~^^ ERROR: mismatched number of components
        uint8 y = 300; //~ ERROR: mismatched types
    }

    function abiDecodeTrailingEmptyType(bytes memory data) public {
        uint256 x = abi.decode(data, (uint256,));
        //~^ ERROR: `abi.decode` type tuple components cannot be empty
        //~^^ ERROR: mismatched number of components
        uint8 y = 300; //~ ERROR: mismatched types
    }
}
