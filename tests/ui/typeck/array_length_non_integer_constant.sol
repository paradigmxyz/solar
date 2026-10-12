contract C {
    bytes1 constant S = "a";
    bool constant T = true;

    uint256[T] t; //~ ERROR: unsupported expression

    function f() external pure {
        abi.decode("", (uint256[S])); //~ ERROR: unsupported literal
    }
}
