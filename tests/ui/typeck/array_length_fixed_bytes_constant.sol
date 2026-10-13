contract C {
    bytes1 constant B = 0x02;
    bytes1 constant S = "a";

    uint256[B] a; //~ ERROR: `bytesN` constants are not integers
    uint256[S] s; //~ ERROR: `bytesN` constants are not integers

    function f() external pure {
        uint256[B] memory b; //~ ERROR: `bytesN` constants are not integers
        b;
        abi.decode("", (uint256[B])); //~ ERROR: `bytesN` constants are not integers
    }
}
