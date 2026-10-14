contract C {
    bytes1 constant B = 0x02;
    bytes1 constant S = "a";

    uint256[B] a; //~ ERROR: failed to evaluate constant: expected an integer
    uint256[S] s; //~ ERROR: failed to evaluate constant: expected an integer

    function f() external pure {
        uint256[B] memory b; //~ ERROR: failed to evaluate constant: expected an integer
        b;
        abi.decode("", (uint256[B])); //~ ERROR: failed to evaluate constant: expected an integer
    }
}
