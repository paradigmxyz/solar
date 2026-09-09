//@ codegen-matrix: standard
//@ run-call: LowFmpAllocation::probe 128 => 37, 0
//@ run-call: LowFmpAllocation::returnedBuffer => 0
//@ run-call: LowFmpAllocation::returnedBufferHash => 0x012893657d8eb2efad4de0a91bcd0e39ad9837745dec3ea923737ea803fc8e3d
//@ run-call: LowFmpAllocation::returnedBufferSecond => 0x012893657d8eb2efad4de0a91bcd0e39ad9837745dec3ea923737ea803fc8e3d
//@ run-call: LowFmpArguments::output => 0x012893657d8eb2efad4de0a91bcd0e39ad9837745dec3ea923737ea803fc8e3d
//@ run-call: MultiReturnUnbumpedMemory::output => 0x1234000000000000000000000000000000000000000000000000000000000000

contract MultiReturnUnbumpedMemory {
    function output() external pure returns (bytes32 result) {
        assembly {
            function emitWord(cursor) -> next, ignored {
                mstore(cursor, shl(240, 0x1234))
                next := add(cursor, 2)
            }

            let start := add(mload(0x40), 0x20)
            let cursor := start
            let ignored
            cursor, ignored := emitWord(cursor)
            result := mload(start)
        }
    }
}

contract LowFmpAllocation {
    function probe(uint256 size) external pure returns (uint256, uint256) {
        assembly { mstore(0x40, 0x80) }
        return allocate(37, size & 255);
    }

    function returnedBuffer() external pure returns (uint256 word) {
        assembly { mstore(0x40, 0x80) }
        bytes memory data = buffer(128);
        assembly { word := mload(add(data, 32)) }
    }

    function returnedBufferHash() external pure returns (bytes32) {
        assembly { mstore(0x40, 0x80) }
        bytes memory data = buffer(128);
        require(increment(41) == 42);
        return keccak256(data);
    }

    function returnedBufferSecond() external pure returns (bytes32) {
        assembly { mstore(0x40, 0x80) }
        bytes memory data = buffer(128);
        (, uint256 second, uint256 third) = triple(41);
        require(second == 42 && third == 43);
        return keccak256(data);
    }

    function triple(uint256 value) internal pure returns (uint256, uint256, uint256) {
        return (value, value + 1, value + 2);
    }

    function increment(uint256 value) internal pure returns (uint256) {
        return value + 1;
    }

    function buffer(uint256 size) internal pure returns (bytes memory) {
        return new bytes(size);
    }

    function allocate(uint256 saved, uint256 size) internal pure returns (uint256, uint256 word) {
        bytes memory data = new bytes(size);
        assembly { word := mload(add(data, 32)) }
        return (saved, word);
    }
}

contract LowFmpArguments {
    function output() external pure returns (bytes32) {
        bytes memory cursor;
        assembly {
            let saved := mload(0x40)
            mstore(0x40, 0x80)
            cursor := mload(0x40)
            mstore(0x40, saved)
        }
        bytes memory data = allocate(cursor);
        return keccak256(data);
    }

    function allocate(bytes memory cursor) internal pure returns (bytes memory) {
        assembly { mstore(0x40, cursor) }
        return new bytes(128);
    }
}
