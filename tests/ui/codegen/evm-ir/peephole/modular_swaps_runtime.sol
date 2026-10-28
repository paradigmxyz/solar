//@ codegen-matrix: standard
//@ run-call: addMul 7, 11, 13, 17 => 5, 5
//@ run-call: addMul 7, 11, 13, 1 => 0, 0
//@ run-call: addMul 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 13 => 6, 6
//@ run-call: mulAdd 7, 11, 13, 17 => 13, 13
//@ run-call: mulAdd 7, 11, 13, 1 => 0, 0
//@ run-call: mulAdd 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 13 => 8, 8
//@ run-call: mulMul 7, 11, 13, 17 => 15, 15
//@ run-call: mulMul 7, 11, 13, 1 => 0, 0
//@ run-call: mulMul 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 13 => 8, 8

contract ModularSwaps {
    // The modular operations read their first two operands symmetrically, so the scheduler's
    // `SWAP1` before them is dead and the second result equals the first.
    function addMul(uint256 a, uint256 b, uint256 c, uint256 n) public pure returns (uint256 x, uint256 y) {
        x = addmod(mulmod(a, b, n), c, n);
        y = addmod(c, mulmod(a, b, n), n);
    }

    function mulAdd(uint256 a, uint256 b, uint256 c, uint256 n) public pure returns (uint256 x, uint256 y) {
        x = mulmod(addmod(a, b, n), c, n);
        y = mulmod(c, addmod(a, b, n), n);
    }

    function mulMul(uint256 a, uint256 b, uint256 c, uint256 n) public pure returns (uint256 x, uint256 y) {
        x = mulmod(mulmod(a, b, n), c, n);
        y = mulmod(c, mulmod(a, b, n), n);
    }
}
