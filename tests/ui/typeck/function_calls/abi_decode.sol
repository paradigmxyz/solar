contract C {
    struct Unsupported {
        mapping(uint256 => uint256) values;
    }

    function test(bytes memory data) public pure {
        abi.decode(data, ());

        abi.decode(); //~ ERROR: wrong argument count for function call
        abi.decode(data); //~ ERROR: wrong argument count for function call
        abi.decode(data, uint256, uint256); //~ ERROR: wrong argument count for function call
        //~^ ERROR: the second argument to `abi.decode` must be a tuple of types

        abi.decode(uint256, uint256); //~ ERROR: mismatched types
        //~^ ERROR: the second argument to `abi.decode` must be a tuple of types

        abi.decode(data, (type(uint256))); //~ ERROR: `abi.decode` type tuple components must be types
        abi.decode(data, ((uint256, int256))); //~ ERROR: `abi.decode` type tuple components must be types
        abi.decode(data, (Unsupported)); //~ ERROR: decoding type not supported
    }
}
