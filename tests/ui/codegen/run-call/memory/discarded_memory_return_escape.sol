//@ compile-flags: -Osize
//@ run-call: run 32 => 141

contract DiscardedMemoryReturnEscape {
    uint256 private escaped;

    function run(uint256 length) external returns (uint256 result) {
        escape(0, length);
        bytes memory overwrite = new bytes(length + 1024);
        assembly {
            mstore(add(overwrite, 32), 99)
            result := add(mload(add(sload(escaped.slot), 32)), mload(add(overwrite, 32)))
        }
    }

    // The return is discarded as a Solidity value, but inline assembly makes
    // its pointer observable after the call.
    function escape(uint256 depth, uint256 length) internal returns (bytes memory value) {
        if (depth != 0) return escape(depth - 1, length);
        value = new bytes(length);
        assembly {
            mstore(add(value, 32), 42)
            sstore(escaped.slot, value)
        }
    }
}
