//@ codegen-matrix: standard ir
//@[ir] compile-flags: -Zdump=evm-ir-runtime
//@[ir] filecheck:
//@ run-call: packed 0, 0 => 0
//@ run-call: packed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 255
//@ run-call: packed 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 4 => 1020
//@ run-call: complement 7, 11, 0 => 7
//@ run-call: complement 7, 11, 1 => 4
//@ run-call: complement 7, 11, 3 => 5
//@ run-call: complement 7, 11, 4 => 9
//@ run-call: complement 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 0, 2 => 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
//@ run-call: rotate 2, 3, 5, 0 => 2, 3, 5
//@ run-call: rotate 2, 3, 5, 1 => 1, 8, 5
//@ run-call: rotate 2, 3, 5, 4 => 23, 27, 11
contract LoopPhiOrder {
    // CHECK-LABEL: @module LoopPhiOrder_runtime
    // The accumulator is consumed directly after BYTE, without shuffling expired phis.
    // CHECK: byte
    // CHECK-NEXT: add
    function packed(uint256 x, uint256 rounds) public pure returns (uint256 result) {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                result := add(result, and(shr(160, x), 255))
                x := add(shl(1, x), i)
            }
        }
    }

    function complement(uint256 x, uint256 y, uint256 rounds) public pure returns (uint256 result) {
        result = x;
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                result := sub(y, result)
                y := add(y, i)
            }
        }
    }

    function rotate(uint256 x, uint256 y, uint256 z, uint256 rounds)
        public pure returns (uint256, uint256, uint256)
    {
        assembly {
            for { let i := 0 } lt(i, rounds) { i := add(i, 1) } {
                x := xor(x, y)
                y := add(y, z)
                z := add(z, i)
            }
        }
        return (x, y, z);
    }
}
