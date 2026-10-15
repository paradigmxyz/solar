contract C {
    bytes1 constant B = 0x02;
    bytes1 constant S = "a";

    uint256[B] a; //~ ERROR: failed to evaluate constant: unsupported expression
    uint256[S] s; //~ ERROR: failed to evaluate constant: unsupported expression

    function f() external pure {
        uint256[B] memory b; //~ ERROR: failed to evaluate constant: unsupported expression
        b;
        abi.decode("", (uint256[B])); //~ ERROR: failed to evaluate constant: unsupported expression
    }
}
