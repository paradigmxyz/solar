// A length that does not evaluate to an integer is reported once.

contract C {
    bool constant T = true;
    bytes1 constant S = "a";

    uint256[T] t; //~ ERROR: failed to evaluate constant: expected an integer
    uint256["lol"] l; //~ ERROR: failed to evaluate constant: expected an integer
    uint256[S] s; //~ ERROR: failed to evaluate constant: expected an integer

    function f() external pure {
        abi.decode("", (uint256[S])); //~ ERROR: failed to evaluate constant: expected an integer
    }
}
