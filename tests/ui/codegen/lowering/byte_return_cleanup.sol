//@ codegen-matrix: standard
//@ run-call: extract 0xab000000000000000000000000000000000000000000000000000000000000 => 0xab, 0
//@ run-call: extract 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0xff, 0
//@ run-call: extract 1 => 0x00, 0
contract ByteReturnCleanup {
    function extract(uint256 x) external pure returns (bytes1 result, uint256 zero) {
        assembly { result := shl(248, and(shr(240, x), 255)) }
    }
}
