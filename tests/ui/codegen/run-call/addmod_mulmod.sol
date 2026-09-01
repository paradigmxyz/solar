//@ run-call: am 1, 2, 3 => 0
//@ run-call: am 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2, 3 => 2
//@ run-call-fail: am 0, 0, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: mm 5, 5, 7 => 4
//@ run-call-fail: mm 0, 0, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: yulAm 0, 0, 0 => 0
//@ run-call: yulMm 0, 0, 0 => 0

contract AddmodMulmod {
    function am(uint256 x, uint256 y, uint256 modulus) external pure returns (uint256) {
        return addmod(x, y, modulus);
    }

    function mm(uint256 x, uint256 y, uint256 modulus) external pure returns (uint256) {
        return mulmod(x, y, modulus);
    }

    function yulAm(uint256 x, uint256 y, uint256 modulus) external pure returns (uint256 result) {
        assembly {
            result := addmod(x, y, modulus)
        }
    }

    function yulMm(uint256 x, uint256 y, uint256 modulus) external pure returns (uint256 result) {
        assembly {
            result := mulmod(x, y, modulus)
        }
    }
}
