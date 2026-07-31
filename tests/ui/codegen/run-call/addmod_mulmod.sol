//@ run-call: am(uint256,uint256,uint256) 1, 2, 3 => 0
//@ run-call: am(uint256,uint256,uint256) 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2, 3 => 2
//@ run-call-fail: am(uint256,uint256,uint256) 0, 0, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: mm(uint256,uint256,uint256) 5, 5, 7 => 4
//@ run-call-fail: mm(uint256,uint256,uint256) 0, 0, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012

contract AddmodMulmod {
    function am(uint256 x, uint256 y, uint256 modulus) external pure returns (uint256) {
        return addmod(x, y, modulus);
    }

    function mm(uint256 x, uint256 y, uint256 modulus) external pure returns (uint256) {
        return mulmod(x, y, modulus);
    }
}
