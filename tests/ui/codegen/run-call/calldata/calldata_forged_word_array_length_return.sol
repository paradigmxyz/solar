//@ codegen-matrix: standard
//@ run-call: 0xdd29ebdb00000000000000000000000000000000000000000000000000000000000000040800000000000000000000000000000000000000000000000000000000000000 => 0x00000000000000000000000000000000000000000000000000000000000000200800000000000000000000000000000000000000000000000000000000000000

// Assembly sets the length of a returned calldata array of full words to `2**251`, so that its
// byte size wraps to zero. We return the length word and no data, where solc reverts (CODEGEN-027).
contract CalldataForgedWordArrayLengthReturn {
    function forged(uint256 offset, uint256 length) external pure returns (uint256[] calldata p) {
        assembly {
            p.offset := offset
            p.length := length
        }
    }
}
