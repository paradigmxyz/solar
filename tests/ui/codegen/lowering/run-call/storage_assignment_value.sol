//@ codegen-matrix: standard
//@ run-call: assignmentValue => 0x616263, 1
//@ run-call: assignmentLength => 4
//@ run-call: assignmentStorage => 0x62
//@ run-call: assignmentStruct => 0x616263
//@ run-call: assignmentCalldata 0x616263 => 0x616263

contract StorageAssignmentValue {
    struct State {
        bytes data;
    }

    State internal state;
    uint256 private accesses;

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
