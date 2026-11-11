// Widths the e-graph reads from definitions, the exponent bound of a constant
// base, and the word bounds of fused boolean tests, each proved from the
// definitions of the values they are about.
contract Widths {
    function divided(uint8 a) external pure returns (uint256) {
        return uint256(a) / 300;
    }

    function masked(uint8 a) external pure returns (uint256) {
        return uint256(a) & 0xffff;
    }

    function power(uint8 e) external pure returns (uint256) {
        return 2 ** e;
    }

    function bounded_power(uint256 e) external pure returns (uint256) {
        require(e < 100);
        return 3 ** e;
    }

    function both(bool a, bool b) external pure returns (bool) {
        return a && b;
    }

    function narrowed(uint8 a, uint8 b) external pure returns (bool) {
        return (uint256(a) << 8) == (uint256(b) << 8);
    }
}
