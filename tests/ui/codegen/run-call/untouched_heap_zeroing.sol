//@ codegen-matrix: standard
//@ run-call: entryArray 5 => 0
//@ run-call: entryBytes 70 => 0
//@ run-call: entryFixed 3 => 0
//@ run-call: afterEmit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 4 => 0
//@ run-call: afterCallStaging 4 => 0
//@ run-call: afterAssemblyWrite 4 => 0
//@ run-call: afterAssemblyReserve 4 => 0
//@ run-call: afterCallOutput 4 => 0
//@ run-call: afterRewind 4 => 0
//@ run-call-fail: entryArray 1099511627776
//@ run-call-fail: entryBytes 1099511627776
//@ run-call: loopEmit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 3 => 0
//@ run-call: afterInternalStaging 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 4 => 0
//@ run-call: decodeTail 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000021aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa00000000000000000000000000000000000000000000000000000000000000 => 0
//@ run-call: literalAtEntry => 0x4141414141414141414141414141414141414141414141414141414141414141414141414141414141
//@ run-call: literalAfterEmit 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff => 0x4141414141414141414141414141414141414141414141414141414141414141414141414141414141
//@ run-call: decodeTailAfterEmit 0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000021aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa00000000000000000000000000000000000000000000000000000000000000 => 0

// Memory above the free memory pointer is zero until something writes there. Allocation skips the
// zero fill of such memory, and a decoded `bytes` its zero padding, and keeps both once a write
// at or above the pointer may have dirtied the memory. Every function reads back what the
// allocation must have zeroed.
contract Test {
    event Data(bytes data);

    function big() external pure returns (uint256[8] memory words) {
        for (uint256 i = 0; i < 8; i++) {
            words[i] = type(uint256).max;
        }
    }

    function or(uint256[] memory values) internal pure returns (uint256 acc) {
        for (uint256 i = 0; i < values.length; i++) {
            acc |= values[i];
        }
    }

    function orBytes(bytes memory values) internal pure returns (uint256 acc) {
        for (uint256 i = 0; i < values.length; i++) {
            acc |= uint8(values[i]);
        }
    }

    // Nothing wrote the heap yet.
    function entryArray(uint256 n) external pure returns (uint256) {
        return or(new uint256[](n));
    }

    function entryBytes(uint256 n) external pure returns (uint256) {
        return orBytes(new bytes(n));
    }

    function entryFixed(uint256 i) external pure returns (uint256) {
        uint256[8] memory words;
        return words[i];
    }

    // The event stages its data at the pointer.
    function afterEmit(bytes calldata data, uint256 n) external returns (uint256) {
        emit Data(data);
        return or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // The call stages its input and output at the pointer.
    function afterCallStaging(uint256 n) external view returns (uint256) {
        uint256[8] memory words = this.big();
        return words[0] & 0 | or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // Assembly writes at the pointer without reserving.
    function afterAssemblyWrite(uint256 n) external pure returns (uint256) {
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(ptr, not(0))
            mstore(add(ptr, 0x20), not(0))
            mstore(add(ptr, 0x40), not(0))
        }
        return or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // A call writes its output at the pointer without reserving it.
    function afterCallOutput(uint256 n) external view returns (uint256) {
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(0, shl(224, 0x887b13a3))
            pop(staticcall(gas(), address(), 0, 4, ptr, 0x100))
        }
        return or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // The pointer moves back over an allocation that holds data.
    function afterRewind(uint256 n) external pure returns (uint256) {
        uint256[] memory old = new uint256[](4);
        old[0] = type(uint256).max;
        old[3] = type(uint256).max;
        assembly ("memory-safe") {
            mstore(0x40, old)
        }
        return or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // Assembly reserves the words it writes.
    function afterAssemblyReserve(uint256 n) external pure returns (uint256) {
        assembly ("memory-safe") {
            let ptr := mload(0x40)
            mstore(0x40, add(ptr, 0x40))
            mstore(ptr, not(0))
            mstore(add(ptr, 0x20), not(0))
        }
        return or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // Each iteration allocates over the event data of the one before.
    function loopEmit(bytes calldata data, uint256 n) external returns (uint256 acc) {
        for (uint256 i = 0; i < n; i++) {
            emit Data(data);
            uint256[] memory words = new uint256[](4);
            acc |= or(words);
            words[0] = type(uint256).max;
        }
    }

    function stage(bytes memory data) internal {
        emit Data(data);
    }

    // The callee stages data above the pointer.
    function afterInternalStaging(bytes calldata data, uint256 n) external returns (uint256) {
        stage(data);
        return or(new uint256[](n)) | orBytes(new bytes(n * 32));
    }

    // The padding after the decoded bytes is zero, and the bytes after it too.
    function decodeTail(bytes calldata raw) external pure returns (uint256 word) {
        bytes memory data = abi.decode(raw, (bytes));
        assembly ("memory-safe") {
            word := mload(add(add(data, 0x20), mload(data)))
        }
    }

    function literalAtEntry() external pure returns (bytes memory) {
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    }

    // The padding word after the literal is zero whatever the event left there.
    function literalAfterEmit(bytes calldata data) external returns (bytes memory) {
        emit Data(data);
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    }

    function decodeTailAfterEmit(bytes calldata raw) external returns (uint256 word) {
        emit Data(raw);
        bytes memory data = abi.decode(raw, (bytes));
        assembly ("memory-safe") {
            word := mload(add(add(data, 0x20), mload(data)))
        }
    }
}
