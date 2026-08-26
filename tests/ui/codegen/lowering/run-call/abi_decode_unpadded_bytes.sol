//@ codegen-matrix: standard
//@ run-call: AbiDecodeUnpaddedBytes::decode() => 1, 0x7a

contract AbiDecodeUnpaddedBytes {
    function decode() external pure returns (uint256 length, bytes1 first) {
        bytes memory encoded = new bytes(65);
        assembly {
            mstore(add(encoded, 0x20), 0x20)
            mstore(add(encoded, 0x40), 1)
            mstore8(add(encoded, 0x60), 0x7a)
        }
        bytes memory value = abi.decode(encoded, (bytes));
        return (value.length, value[0]);
    }
}
