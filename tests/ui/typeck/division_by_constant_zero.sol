// A division of a constant by a constant zero is an error. A non-constant dividend is not checked.

contract C {
    uint constant Z = 0;
    int8 constant Z8 = 0;
    uint constant ONE = 1;

    function f(uint x) public pure returns (uint, uint, uint, int8, uint) {
        return (
            1 / Z, //~ ERROR: division by zero
            ONE % Z, //~ ERROR: modulo zero
            ONE / (ONE - 1), //~ ERROR: division by zero
            int8(1) / Z8,
            x / Z
        );
    }
}
