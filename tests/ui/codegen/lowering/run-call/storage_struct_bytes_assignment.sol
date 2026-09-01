//@ codegen-matrix: standard
//@ run-call: StorageStructBytesAssignment::roundTrip => 9, 96

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
}
