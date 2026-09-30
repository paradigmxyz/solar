//@ codegen-matrix: standard
//@ compile-flags: -Zvalidate-ir=true
//@ run-call: StorageStructBytesAssignment::roundTrip => 9, 96
//@ run-call: cleanupEmpty => 0, 0, 0
//@ run-call: cleanupShort => 1, 0, 0
//@ run-call: choose true => 0x61
//@ run-call: choose false => 0x62
//@ run-call: assignmentValue => 0x616263, 1
//@ run-call: assignmentLength => 4
//@ run-call: assignmentStorage => 0x62
//@ run-call: assignmentStruct => 0x616263
//@ run-call: assignmentCalldata 0x616263 => 0x616263
//@ run-call-fail: malformed 64 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022
//@ run-call-fail: malformed 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000022

contract StorageStructBytesAssignment {
    struct State {
        bytes data;
    }

    State internal state;
    uint256 private accesses;

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

    function assignmentValue() external returns (bytes memory result, uint256 count) {
        result = (accessState().data = "abc");
        count = accesses;
    }

    function assignmentLength() external returns (uint256) {
        return (state.data = "abcd").length;
    }

    function assignmentStruct() external returns (bytes memory) {
        return (state = State("abc")).data;
    }

    function assignmentCalldata(bytes calldata input) external returns (bytes memory) {
        return (state.data = input);
    }

    function assignmentStorage() external returns (bytes memory) {
        state.data = "a";
        return (accessState().data = state.data);
    }

    function accessState() internal returns (State storage) {
        accesses++;
        state.data = "b";
        return state;
    }
}
