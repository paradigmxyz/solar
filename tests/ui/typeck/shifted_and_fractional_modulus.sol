// solc's constant evaluator gives no value to a shift by more than `2**32 - 1` bits, and evaluates
// `(1 / 2) * 2` exactly to 1, so it reports no arithmetic modulo zero for these moduli.

contract C {
    uint256 constant ONE = 1;

    function shift(uint256 a, uint256 b) public pure returns (uint256) {
        return addmod(a, b, ONE >> 4294967296);
    }

    function maxShift(uint256 a, uint256 b) public pure returns (uint256) {
        return addmod(a, b, ONE >> 4294967295); //~ ERROR: arithmetic modulo zero
    }

    function fraction(uint256 a, uint256 b) public pure returns (uint256) {
        return mulmod(a, b, (1 / 2) * 2);
    }
}
