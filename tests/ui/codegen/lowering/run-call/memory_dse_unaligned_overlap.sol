//@ codegen-matrix: standard
//@ run-call: restore() => 0x0000000000000000000000000000000000000000000000000000000000001234
contract MemoryDseUnalignedOverlap {
    function restore() external pure returns (bytes32 result) {
        assembly ("memory-safe") {
            mstore(0x80, 0x1234)
            mstore(0x81, 0)
            mstore(0x80, 0x1234)
            result := mload(0x80)
        }
    }
}
