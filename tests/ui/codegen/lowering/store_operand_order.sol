//@ codegen-matrix: standard
//@ run-call: memoryWords 0 => 0, 1, 0
//@ run-call: memoryWords 42 => 42, 43, 42
//@ run-call: memoryWords 255 => 255, 256, 255
//@ run-call: stateWords 0 => 0, 1, 0
//@ run-call: stateWords 42 => 42, 43, 42

contract StoreOperands {
    function memoryWords(uint256 seed) external pure returns (uint256 old, uint256 word, uint256 byteWord) {
        assembly {
            let p := mload(0x40)
            mstore(p, seed)
            old := mload(p)
            // The previously read SSA word remains live across an aliasing write.
            mstore(p, add(old, 1))
            mstore8(add(p, 63), old)
            word := mload(p)
            byteWord := and(mload(add(p, 32)), 255)
            mstore(0x40, add(p, 64))
        }
    }

    function stateWords(uint256 seed) external returns (uint256 old, uint256 word, uint256 transientWord) {
        assembly {
            sstore(seed, seed)
            old := sload(seed)
            sstore(seed, add(old, 1))
            tstore(seed, old)
            word := sload(seed)
            transientWord := tload(seed)
        }
    }
}
