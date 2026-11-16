//@ codegen-matrix: standard
//@ run-call: addNegative 1 => 9
//@ run-call: addNegativeFirst 1 => 9
//@ run-call: byteStore 1 => 9
//@ run-call: callOutput 1 => 9

// Each helper returns a pointer 160 bytes before the heap, and the caller
// writes there while a value stays live across the call. Codegen must see how
// far back the pointer reaches and keep that much room below the heap, so the
// write cannot land in the spill slot that holds the live value. The first two
// helpers add a negative constant, in either operand order.
contract HeapPrefixShapes {
    function addNegative(uint256 seed) external pure returns (uint256 result) {
        assembly {
            function pair() -> first, second {
                first := add(mload(0x40), not(159))
                second := 7
            }
            let live := add(seed, 1)
            let first, second := pair()
            mstore(first, 999)
            result := add(live, second)
        }
    }

    function addNegativeFirst(uint256 seed) external pure returns (uint256 result) {
        assembly {
            function pair() -> first, second {
                first := add(not(159), mload(0x40))
                second := 7
            }
            let live := add(seed, 1)
            let first, second := pair()
            mstore(first, 999)
            result := add(live, second)
        }
    }

    // Any write counts, whatever its width or source.
    function byteStore(uint256 seed) external pure returns (uint256 result) {
        assembly {
            function pair() -> first, second {
                first := sub(mload(0x40), 160)
                second := 7
            }
            let live := add(seed, 1)
            let first, second := pair()
            mstore8(first, 0xff)
            result := add(live, second)
        }
    }

    function callOutput(uint256 seed) external view returns (uint256 result) {
        assembly {
            function pair() -> first, second {
                first := sub(mload(0x40), 160)
                second := 7
            }
            let live := add(seed, 1)
            let first, second := pair()
            mstore(0, 0xdead)
            pop(staticcall(gas(), 4, 0, 32, first, 32))
            result := add(live, second)
        }
    }
}
