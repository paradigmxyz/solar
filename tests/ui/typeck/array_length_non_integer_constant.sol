// A length that does not evaluate to an integer is reported once.

contract C {
    bool constant T = true;
    bytes1 constant S = "a";

    uint256[T] t; //~ ERROR: failed to evaluate constant
    uint256["lol"] l; //~ ERROR: failed to evaluate constant
    uint256[S] s; //~ ERROR: failed to evaluate constant
}
