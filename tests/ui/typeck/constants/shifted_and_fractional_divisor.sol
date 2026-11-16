// solc's constant evaluator gives no value to a shift by more than `2**32 - 1` bits, and evaluates
// `(1 / 2) * 2` exactly to 1, so it reports no division or modulo by zero for these operands.

contract C {
    uint256 constant ONE = 1;

    function shiftedDivisor() public pure returns (uint256) {
        return 1 / (ONE >> 4294967296);
    }

    function shiftedDividend(uint256 x) public pure returns (uint256) {
        return x == 0 ? 7 : (ONE >> 4294967296) % 0;
    }

    function maxShift() public pure returns (uint256) {
        return 1 / (ONE >> 4294967295); //~ ERROR: division by zero
    }

    function fraction() public pure returns (uint256) {
        return 1 % ((1 / 2) * 2);
    }
}
