//@ignore-host: windows
//@compile-flags: -Zcodegen -Zdump=mir

// Packed encoding writes each value's top `size` bytes: fixed-bytes values
// are already left-aligned and must not be shifted again, and `bytes`/
// `string` values copy their data without padding (runtime-length cursor).
contract AbiEncodePackedMixed {
    function fixedBytesArg(uint a, address b, bytes2 c) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(a, b, c));
    }

    function dynamicArg(bytes32 h, bytes memory tail) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(h, tail));
    }

    function materialized(uint16 a, bytes memory mid, bool b) external pure returns (bytes memory) {
        return abi.encodePacked(a, mid, b);
    }
}
