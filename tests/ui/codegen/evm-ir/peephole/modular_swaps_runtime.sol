//@ codegen-matrix: standard
//@ run-call: compute 7, 11, 13 => 5, 12, 13
//@ run-call: compute 7, 11, 1 => 0, 0, 1
//@ run-call: compute 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 13 => 4, 4, 13
//@ run-call: compute 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0, 0, 0

contract ModularSwaps {
    function compute(uint256 x, uint256 y, uint256 modulus)
        public pure returns (uint256 sum, uint256 product, uint256 retainedModulus)
    {
        // Preserve the modulus and exercise the full-width EVM operations, including a zero
        // modulus and intermediates wider than 256 bits.
        assembly {
            sum := addmod(x, y, modulus)
            product := mulmod(x, y, modulus)
        }
        retainedModulus = modulus;
    }
}
