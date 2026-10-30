//@ codegen-matrix: standard
//@ run-call: preserveDestination => 7
//@ run-call: preserveDynamicDestination 0x => 7
//@ run-call: preserveDynamicDestination 0x1234 => 7
//@[mir] normalize-stdout-test: "(?s).+" -> ""

contract Test {
    event Words(uint256 a, uint256 b, uint256 c);
    event Data(bytes data);

    // The event borrows the free-memory pointer, so dropping its update overwrites live data.
    function preserveDestination() external returns (uint256) {
        assembly {
            mstore(0x80, 7)
            mstore(0x40, 0x100)
        }
        emit Words(42, 43, 44);
        assembly {
            mstore(0, mload(0x80))
            return(0, 32)
        }
    }

    // Dynamic calldata encodings borrow the same destination without a memory source read.
    function preserveDynamicDestination(bytes calldata data) external returns (uint256) {
        assembly {
            mstore(0x80, 7)
            mstore(0x40, 0x100)
        }
        emit Data(data);
        assembly {
            mstore(0, mload(0x80))
            return(0, 32)
        }
    }
}
