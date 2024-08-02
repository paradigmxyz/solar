contract C {
    uint256 transient;

    function f() external {
        transient = 1;
    }
}

contract D {
    uint256 transient transient;

    function f() external {
        transient = 1;
    }
}
