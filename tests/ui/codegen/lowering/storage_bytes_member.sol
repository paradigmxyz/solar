//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime

// A `bytes` struct field reached through a storage reference bound from a
// mapping element (`state.part` below) uses Solidity's packed short/long
// storage-bytes form. All of these operations must route through that form:
// `push`/`pop` (previously rejected with "does not support this `.push`
// member call"), element read/write, `.length`, and using the field as a
// value (which materializes a `[length][data...]` memory copy — previously
// the raw slot word was handed out as if it were a memory pointer).
// Runtime behavior is verified equal to solc 0.8.30 separately, across the
// 31/32-byte short/long form boundary (nitro-contracts HashProofHelper).

contract StorageBytesMember {
    struct KeccakState {
        uint64 offset;
        bytes part;
        uint256 length;
    }

    mapping(address => KeccakState) states;

    function pushRange(uint8 from, uint8 count) external {
        KeccakState storage state = states[msg.sender];
        for (uint256 i = 0; i < count; i++) {
            state.part.push(bytes1(uint8(from + i)));
        }
    }

    function popOne() external {
        KeccakState storage state = states[msg.sender];
        state.part.pop();
    }

    function len() external view returns (uint256) {
        KeccakState storage state = states[msg.sender];
        return state.part.length;
    }

    function at(uint256 i) external view returns (bytes1) {
        KeccakState storage state = states[msg.sender];
        return state.part[i];
    }

    function setAt(uint256 i, bytes1 b) external {
        KeccakState storage state = states[msg.sender];
        state.part[i] = b;
    }

    function whole() external view returns (bytes memory) {
        KeccakState storage state = states[msg.sender];
        return state.part;
    }
}
