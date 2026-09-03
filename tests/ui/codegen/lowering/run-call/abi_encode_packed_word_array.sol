//@ codegen-matrix: standard
//@ run-call: pack => 0xe90b7bceb6e7df5418fb78d8ee546e97c83a08bbccc01a0644d599ccd2a7c2e0, 64
//@ run-call: cleanDirtyMemory => 1
//@ run-call: rejectDirtyCalldata => 1

contract AbiEncodePackedWordArray {
    function pack() external pure returns (bytes32 digest, uint256 len) {
        bytes32[] memory values = new bytes32[](2);
        values[0] = bytes32(uint256(1));
        values[1] = bytes32(uint256(2));
        bytes memory encoded = abi.encodePacked(values);
        return (keccak256(encoded), encoded.length);
    }

    function cleanDirtyMemory() external pure returns (uint256) {
        uint8[] memory values = new uint8[](2);
        assembly {
            mstore(add(values, 0x20), 0x101)
            mstore(add(values, 0x40), 0x202)
        }
        bytes memory encoded = abi.encodePacked(values);
        uint256 first;
        uint256 second;
        assembly {
            first := mload(add(encoded, 0x20))
            second := mload(add(encoded, 0x40))
        }
        require(encoded.length == 64 && first == 1 && second == 2, "dirty memory");
        return 1;
    }

    function packCalldata(uint8[] calldata values) external pure returns (bytes32) {
        return keccak256(abi.encodePacked(values));
    }

    function rejectDirtyCalldata() external returns (uint256) {
        uint8[] memory values = new uint8[](1);
        values[0] = 1;
        bytes memory payload = abi.encodeWithSelector(this.packCalldata.selector, values);
        assembly {
            // bytes header + selector + offset + length = 100 bytes.
            mstore(add(payload, 100), 0x101)
        }
        (bool success,) = address(this).call(payload);
        require(!success, "dirty calldata");
        return 1;
    }
}
