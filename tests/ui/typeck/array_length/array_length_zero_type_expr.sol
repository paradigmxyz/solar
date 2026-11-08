contract C {
    function f(bytes calldata input) external pure {
        abi.decode(input, (uint256[0])); //~ ERROR: array length must be greater than zero
    }
}
