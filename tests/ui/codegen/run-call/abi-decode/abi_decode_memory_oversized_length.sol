//@ codegen-matrix: standard
//@ run-call: wordArray => true
//@ run-call: narrowArray => true
//@ run-call: bytesValue => true
//@ run-call: nestedArray => true

contract AbiDecodeMemoryOversizedLength {
    function decodeWordArray(uint256[] memory value) external pure returns (uint256) {
        return value.length;
    }

    function decodeNarrowArray(uint8[] memory value) external pure returns (uint256) {
        return value.length;
    }

    function decodeBytes(bytes memory value) external pure returns (uint256) {
        return value.length;
    }

    function decodeNestedArray(uint256[][] memory value) external pure returns (uint256) {
        return value.length;
    }

    function wordArray() external returns (bool) {
        return oversized(this.decodeWordArray.selector);
    }

    function narrowArray() external returns (bool) {
        return oversized(this.decodeNarrowArray.selector);
    }

    function bytesValue() external returns (bool) {
        return oversized(this.decodeBytes.selector);
    }

    function nestedArray() external returns (bool) {
        return oversized(this.decodeNestedArray.selector);
    }

    function oversized(bytes4 selector) internal returns (bool) {
        bytes memory input = abi.encodePacked(selector, uint256(32), uint256(type(uint64).max));
        (bool ok, bytes memory data) = address(this).call(input);
        return !ok && keccak256(data) == keccak256(abi.encodeWithSignature("Panic(uint256)", 0x41));
    }
}
