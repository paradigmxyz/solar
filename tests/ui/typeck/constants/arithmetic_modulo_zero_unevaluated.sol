// A modulus is zero only where solc's constant evaluator gives it a value. It does not fold
// ternaries, gives no value to a right shift by more than `2**32 - 1` bits, and converts the right
// operand of a typed shift or power to the left operand's type.

contract C {
    uint256 constant ONE = 1;
    uint8 constant ONE8 = 1;
    uint8 constant ZERO8 = 0;
    uint256 constant ZERO = true ? 0 : 1;

    function ternary(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (addmod(a, b, true ? 0 : 1), mulmod(a, b, ZERO));
    }

    function shift(uint256 a, uint256 b) public pure returns (uint256) {
        return addmod(a, b, ONE >> 4294967296);
    }

    function maxShift(uint256 a, uint256 b) public pure returns (uint256) {
        return addmod(a, b, ONE >> 4294967295); //~ ERROR: arithmetic modulo zero
    }

    function narrow(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (addmod(a, b, ONE8 >> 256), mulmod(a, b, ZERO8 ** 256));
    }

    function fraction(uint256 a, uint256 b) public pure returns (uint256, uint256) {
        return (
            mulmod(a, b, (1 / 2) * 2),
            mulmod(a, b, (1 / 2) * 0) //~ ERROR: arithmetic modulo zero
        );
    }
}
