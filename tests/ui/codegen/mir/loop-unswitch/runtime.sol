//@ codegen-matrix: standard
//@ run-call: compute 7, 0, 0 => 7
//@ run-call: compute 7, 0, 1 => 4
//@ run-call: compute 7, 0, 20 => 4
//@ run-call: compute 7, 1, 1 => 8
//@ run-call: compute 7, 1, 20 => 8
//@ run-call: compute 7, 2, 2 => 15
//@ run-call: compute 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: compute 0, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 1
//@ run-call-fail: compute 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 2, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call-fail: compute 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
contract Unswitch {
    function compute(uint256 a, uint256 b, uint256 iterations) external pure returns (uint256 value) {
        value = a;
        for (uint256 i; i < iterations; ++i) {
            value = (value * b + a) / 2;
            value = value % 1000000 + 1;
        }
    }
}
