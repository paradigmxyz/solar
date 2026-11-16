//@ codegen-matrix: standard
//@ run-call: StorageStructBytesAssignment::roundTrip => 9, 96
//@ run-call: cleanupEmpty => 0, 0, 0
//@ run-call: cleanupShort => 1, 0, 0
//@ run-call: choose true => 0x61
//@ run-call: choose false => 0x62
//@ run-call-fail: malformed 64 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@ run-call-fail: malformed 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022

contract StorageStructBytesAssignment {
    struct State {
        bytes data;
    }

    State internal state;

    function roundTrip() external returns (uint256 first, uint256 length) {
        state.data = abi.encodePacked(uint256(9), uint256(5), uint256(1));
        bytes memory data = state.data;
        assembly {
            first := mload(add(data, 0x20))
        }
        length = data.length;
    }

    function cleanupEmpty() external returns (uint256 header, uint256 first, uint256 second) {
        state = State("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        state = State("");
        assembly {
            header := sload(state.slot)
            mstore(0, state.slot)
            let base := keccak256(0, 32)
            first := sload(base)
            second := sload(add(base, 1))
        }
    }

    function cleanupShort() external returns (uint256 length, uint256 first, uint256 second) {
        state = State("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        state = State("a");
        assembly {
            mstore(0, state.slot)
            let base := keccak256(0, 32)
            first := sload(base)
            second := sload(add(base, 1))
        }
        length = state.data.length;
    }

    function choose(bool first) external returns (bytes memory) {
        if (first) state = State("a");
        else state = State("b");
        return state.data;
    }

    function malformed(uint256 header) external {
        assembly { sstore(state.slot, header) }
        state = State("a");
    }
}
