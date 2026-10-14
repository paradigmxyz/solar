contract C {
    bytes1 constant S = "a";
    bool constant T = true;

    uint256[T] t; //~ ERROR: failed to evaluate constant: expected an integer

    function f() external pure {
        abi.decode("", (uint256[S])); //~ ERROR: failed to evaluate constant: expected an integer
    }
}
