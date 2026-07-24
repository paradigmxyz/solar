//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
//@ filecheck:

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

    // CHECK: push 0x4407bb95
    // CHECK: eq
    // CHECK-NEXT: push [[PUSH_RANGE:bb[0-9]+]]
    // CHECK: push 0x53b8a6c6
    // CHECK: eq
    // CHECK-NEXT: push [[POP_ONE:bb[0-9]+]]
    // CHECK: push 0x56d88e27
    // CHECK: eq
    // CHECK-NEXT: push [[LEN:bb[0-9]+]]
    // CHECK: push 0x72bd964d
    // CHECK: eq
    // CHECK-NEXT: push [[WHOLE:bb[0-9]+]]
    // CHECK: push 0xbee6975a
    // CHECK: eq
    // CHECK-NEXT: push [[SET_AT:bb[0-9]+]]
    // CHECK: push 0xe0886f90
    // CHECK: eq
    // CHECK-NEXT: push [[AT:bb[0-9]+]]
    // CHECK: [[PUSH_RANGE]]:
    // CHECK: keccak256
    // CHECK: jump [[LOOP:bb[0-9]+]]
    // CHECK: [[LOOP]]:
    // CHECK: push [[LOOP_EXIT:bb[0-9]+]]
    // CHECK-NEXT: jumpi
    // CHECK: jump [[LOOP]]
    // CHECK: [[LOOP_EXIT]]:
    // CHECK: jump [[LOAD_BYTES:bb[0-9]+]]
    // CHECK: [[LOAD_BYTES]]:
    // CHECK: sload
    // CHECK: mcopy
    // CHECK: sstore
    function pushRange(uint8 from, uint8 count) external {
        KeccakState storage state = states[msg.sender];
        for (uint256 i = 0; i < count; i++) {
            state.part.push(bytes1(uint8(from + i)));
        }
    }

    // CHECK: [[POP_ONE]]:
    // CHECK: keccak256
    // CHECK: jump [[LOAD_BYTES]]
    // CHECK: sload
    // CHECK: sstore
    function popOne() external {
        KeccakState storage state = states[msg.sender];
        state.part.pop();
    }

    // CHECK: [[LEN]]:
    // CHECK: keccak256
    // CHECK: jump [[LOAD_BYTES]]
    // CHECK: mload
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK: [[RETURN]]:
    // CHECK: return
    function len() external view returns (uint256) {
        KeccakState storage state = states[msg.sender];
        return state.part.length;
    }

    // CHECK: [[AT]]:
    // CHECK: keccak256
    // CHECK: jump [[LOAD_BYTES]]
    // CHECK: mload
    // CHECK: mload
    // CHECK: jump [[RETURN]]
    function at(uint256 i) external view returns (bytes1) {
        KeccakState storage state = states[msg.sender];
        return state.part[i];
    }

    // CHECK: [[SET_AT]]:
    // CHECK: keccak256
    // CHECK: sload
    // CHECK: sstore
    // CHECK: keccak256
    // CHECK: sload
    // CHECK: sstore
    function setAt(uint256 i, bytes1 b) external {
        KeccakState storage state = states[msg.sender];
        state.part[i] = b;
    }

    // CHECK: [[WHOLE]]:
    // CHECK: keccak256
    // CHECK: jump [[LOAD_BYTES]]
    // CHECK: mcopy
    // CHECK: return
    function whole() external view returns (bytes memory) {
        KeccakState storage state = states[msg.sender];
        return state.part;
    }
}
