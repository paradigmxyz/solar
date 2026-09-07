//@compile-flags: -Zdump=evm-ir-runtime -Zswitch-lowering=perfect
//@ filecheck:

// A `bytes` struct field reached through a storage reference bound from a
// mapping element (`state.part` below) uses Solidity's packed short/long
// storage-bytes form. All of these operations must route through that form:
// `push`/`pop` (previously rejected with "does not support this `.push`
// member call"), element read/write, `.length`, and using the field as a
// value (which materializes a `[length][data...]` memory copy — previously
// the raw slot word was handed out as if it were a memory pointer). Element
// access and `.length` read the header slot directly instead of copying, and
// `push`/`pop` update the value in place.
// Runtime behavior is verified equal to solc 0.8.30 separately, across the
// 31/32-byte short/long form boundary (nitro-contracts HashProofHelper).

contract StorageBytesMember {
    struct KeccakState {
        uint64 offset;
        bytes part;
        uint256 length;
    }

    mapping(address => KeccakState) states;

    // The former shared len/at return is now a compact full-word return in
    // each getter. This measured code-shape policy change still requires both
    // getters to store and return exactly one complete word.
    // The linked checks follow physical block order: loop bodies precede their
    // selector wrappers. A nonzero selector subtraction takes the reject edge.
    // CHECK-LABEL: @module StorageBytesMember_runtime
    // CHECK: push 15
    // CHECK-NEXT: and
    // CHECK-NEXT: indexed_jump [[AT_BUCKET:bb[0-9]+]], [[REJECT:bb[0-9]+]], [[REJECT]], [[REJECT]], [[REJECT]], [[PUSH_BUCKET:bb[0-9]+]], [[POP_BUCKET:bb[0-9]+]], [[LEN_BUCKET:bb[0-9]+]], [[REJECT]], [[REJECT]], [[SET_BUCKET:bb[0-9]+]], [[REJECT]], [[REJECT]], [[WHOLE_BUCKET:bb[0-9]+]], [[REJECT]], [[REJECT]]
    // CHECK: [[LOOP_BODY:bb[0-9]+]]:
    // CHECK: sload
    // CHECK-NOT: mcopy
    // CHECK: jumpi [[BAD_HEADER:bb[0-9]+]], [[PUSH_VALIDATE:bb[0-9]+]]
    // CHECK: [[PUSH_VALIDATE]]:
    // CHECK: jumpi [[PUSH_FORM:bb[0-9]+]], {{bb[0-9]+}}
    // CHECK: [[PUSH_FORM]]:
    // CHECK-NEXT: push 31
    // CHECK: gt
    // CHECK-NEXT: jumpi [[PUSH_LONG:bb[0-9]+]], [[PUSH_SHORT_TEST:bb[0-9]+]]
    // CHECK: [[PUSH_SHORT_TEST]]:
    // CHECK-NEXT: push 31
    // CHECK: eq
    // CHECK-NEXT: jumpi [[PUSH_EXPAND:bb[0-9]+]], [[PUSH_SHORT:bb[0-9]+]]
    // CHECK: [[PUSH_SHORT]]:
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: jump [[LOOP_STEP:bb[0-9]+]]
    // CHECK: [[PUSH_LONG]]:
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    // CHECK-NOT: mcopy
    // CHECK: sload
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: jump [[LOOP_STEP]]
    // CHECK: [[LOOP_STEP]]:
    // CHECK: jump [[LOOP:bb[0-9]+]]
    // CHECK: [[LOOP]]:
    // CHECK: gt
    // CHECK-NEXT: jumpi [[LOOP_BODY]], [[PUSH_DONE:bb[0-9]+]]
    // CHECK: [[PUSH_DONE]]:
    // CHECK-NEXT: stop
    // CHECK: [[REJECT]] [cold]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: [[POP_FORM:bb[0-9]+]]:
    // CHECK-NEXT: push 32
    // CHECK: eq
    // CHECK-NEXT: jumpi [[POP_CONTRACT:bb[0-9]+]], [[POP_OTHER:bb[0-9]+]]
    // CHECK: [[POP_OTHER]]:
    // CHECK: jumpi [[POP_SHORT:bb[0-9]+]], [[POP_LONG:bb[0-9]+]]
    // CHECK: [[POP_LONG]]:
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    // CHECK-NOT: mcopy
    // CHECK: sload
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: jump [[POP_DONE:bb[0-9]+]]
    // CHECK: [[POP_SHORT]]:
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: jump [[POP_DONE]]
    // CHECK: [[POP_DONE]]:
    // CHECK-NEXT: stop
    // CHECK: [[AT_FORM:bb[0-9]+]]:
    // CHECK-NEXT: jumpi [[AT_LONG:bb[0-9]+]], [[AT_WORD:bb[0-9]+]]
    // CHECK: [[AT_LONG]]:
    // CHECK-NOT: mload
    // CHECK: keccak256
    // CHECK-NOT: mload
    // CHECK: jump [[AT_WORD]]
    // CHECK: [[AT_WORD]]:
    // CHECK-NEXT: sload
    // CHECK-NOT: mload
    // CHECK: push 255
    // CHECK-NEXT: and
    // CHECK-NEXT: push 248
    // CHECK-NEXT: shl
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[SET_FORM:bb[0-9]+]]:
    // CHECK-NEXT: jumpi [[SET_LONG:bb[0-9]+]], [[SET_WORD:bb[0-9]+]]
    // CHECK: [[SET_LONG]]:
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    // CHECK-NOT: mcopy
    // CHECK: jump [[SET_WORD]]
    // CHECK: [[SET_WORD]]:
    // CHECK-NOT: mcopy
    // CHECK: sload
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: stop
    // CHECK: [[WHOLE_LOAD:bb[0-9]+]]:
    // CHECK: sload
    // CHECK: mstore
    // CHECK: jump [[WHOLE_LOOP:bb[0-9]+]]
    // CHECK: [[WHOLE_LOOP]]:
    // CHECK: lt
    // CHECK-NEXT: jumpi [[WHOLE_LOAD]], [[WHOLE_LOADED:bb[0-9]+]]
    // CHECK: [[WHOLE_LOADED]]:
    // CHECK: jump [[WHOLE_ENCODE:bb[0-9]+]]
    // CHECK: [[WHOLE_ENCODE]]:
    // CHECK: jumpi [[WHOLE_PAD:bb[0-9]+]], [[WHOLE_RETURN:bb[0-9]+]]
    // CHECK: [[WHOLE_PAD]]:
    // CHECK: mstore
    // CHECK-NEXT: jump [[WHOLE_RETURN]]
    // CHECK: [[WHOLE_RETURN]]:
    // CHECK: mcopy
    // CHECK: return
    // CHECK: [[BAD_HEADER]] [cold]:
    // CHECK: push 34
    // CHECK: revert
    // CHECK: [[PUSH_EXPAND]]:
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: push 65
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: jump [[LOOP_STEP]]
    // CHECK: [[POP_CONTRACT]]:
    // CHECK-NOT: mcopy
    // CHECK: keccak256
    // CHECK-NOT: mcopy
    // CHECK: sload
    // CHECK-NOT: mcopy
    // CHECK: push 62
    // CHECK-NEXT: or
    // CHECK-NOT: mcopy
    // CHECK: sstore
    // CHECK-NEXT: push 0
    // CHECK-NEXT: swap 1
    // CHECK-NEXT: sstore
    // CHECK-NEXT: jump [[POP_DONE]]
    // CHECK: [[WHOLE_LONG:bb[0-9]+]]:
    // CHECK: keccak256
    // CHECK-NEXT: jump [[WHOLE_LOOP]]
    // CHECK: [[AT_BUCKET]]:
    // CHECK-NEXT: push 0xe0886f90
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT]], [[AT:bb[0-9]+]]
    // CHECK: [[AT]]:
    // CHECK: jumpi [[REJECT]], [[AT_HEADER:bb[0-9]+]]
    // CHECK: [[AT_HEADER]]:
    // CHECK-NEXT: caller
    // CHECK: push 32
    // CHECK-NEXT: mstore
    // CHECK: keccak256
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: sload
    // CHECK-NOT: mload
    // CHECK: jumpi [[BAD_HEADER]], [[AT_BOUNDS:bb[0-9]+]]
    // CHECK: [[AT_BOUNDS]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[AT_FORM]], [[BAD_INDEX:bb[0-9]+]]
    // CHECK: [[BAD_INDEX]] [cold]:
    // CHECK: push 50
    // CHECK: revert
    // CHECK: [[PUSH_BUCKET]]:
    // CHECK-NEXT: push 0x4407bb95
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT]], [[PUSH_RANGE:bb[0-9]+]]
    // CHECK: [[PUSH_RANGE]]:
    // CHECK: caller
    // CHECK: keccak256
    // CHECK: jump [[LOOP]]
    // CHECK: [[POP_BUCKET]]:
    // CHECK-NEXT: push 0x53b8a6c6
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT]], [[POP_ONE:bb[0-9]+]]
    // CHECK: [[POP_ONE]]:
    // CHECK-NEXT: caller
    // CHECK: keccak256
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: sload
    // CHECK-NOT: mcopy
    // CHECK: jumpi [[BAD_HEADER]], [[POP_NONEMPTY:bb[0-9]+]]
    // CHECK: [[POP_NONEMPTY]]:
    // CHECK-NEXT: dup 2
    // CHECK-NEXT: jumpi [[POP_FORM]], [[EMPTY_POP:bb[0-9]+]]
    // CHECK: [[EMPTY_POP]] [cold]:
    // CHECK: push 49
    // CHECK: revert
    // CHECK: [[LEN_BUCKET]]:
    // CHECK-NEXT: push 0x56d88e27
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT]], [[LEN:bb[0-9]+]]
    // CHECK: [[LEN]]:
    // CHECK-NEXT: caller
    // CHECK: keccak256
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: sload
    // CHECK-NOT: mload
    // CHECK: jumpi [[BAD_HEADER]], [[LEN_RETURN:bb[0-9]+]]
    // CHECK: [[LEN_RETURN]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[SET_BUCKET]]:
    // CHECK-NEXT: push 0xbee6975a
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT]], [[SET_AT:bb[0-9]+]]
    // CHECK: [[SET_AT]]:
    // CHECK: caller
    // CHECK: keccak256
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: sload
    // CHECK-NOT: mcopy
    // CHECK: jumpi [[BAD_HEADER]], [[SET_BOUNDS:bb[0-9]+]]
    // CHECK: [[SET_BOUNDS]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK-NEXT: lt
    // CHECK-NEXT: jumpi [[SET_FORM]], [[BAD_INDEX]]
    // CHECK: [[WHOLE_BUCKET]]:
    // CHECK-NEXT: push 0x72bd964d
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT]], [[WHOLE:bb[0-9]+]]
    // CHECK: [[WHOLE]]:
    // CHECK: caller
    // CHECK: push 32
    // CHECK-NEXT: mstore
    // CHECK: keccak256
    // CHECK-NEXT: push 1
    // CHECK-NEXT: add
    // CHECK-NEXT: dup 1
    // CHECK-NEXT: sload
    // CHECK: jumpi [[BAD_HEADER]], [[WHOLE_SIZE:bb[0-9]+]]
    // CHECK: [[WHOLE_SIZE]]:
    // CHECK: jumpi {{bb[0-9]+}}, [[WHOLE_ALLOC:bb[0-9]+]]
    // CHECK: [[WHOLE_ALLOC]]:
    // CHECK: mload
    // CHECK: jumpi {{bb[0-9]+}}, [[WHOLE_STORE:bb[0-9]+]]
    // CHECK: [[WHOLE_STORE]]:
    // CHECK: mstore
    // CHECK: jumpi [[WHOLE_LONG]], [[WHOLE_SHORT:bb[0-9]+]]
    // CHECK: [[WHOLE_SHORT]]:
    // CHECK: mstore
    // CHECK-NEXT: jump [[WHOLE_ENCODE]]

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
