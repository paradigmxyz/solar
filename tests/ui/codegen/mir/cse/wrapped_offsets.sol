//@ codegen-matrix: standard
//@ run-call: copy 0x0102, 0x0000, 0 => true
//@ run-call: copy 0x0102, 0x0000, 1 => true
//@ run-call-fail: copy 0x0102, 0x0000, 2 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032
//@ run-call: reset => 9

// The forged length admits a runtime index that wraps from the data into its own header.
// The first store changes that length, so the second iteration must reload it and panic.
// The MIR test exercises CSE alone; this also covers the complete pipelines, where later
// copy transformations must not hide an invalid forwarded length.
contract WrappedOffsets {
    function copy(bytes calldata source, bytes memory destination, uint256 count)
        external pure returns (bool)
    {
        assembly {
            mstore(destination, not(0))
        }
        for (uint256 k; k < count; ++k) {
            destination[type(uint256).max - 31 + k] = source[k];
        }
        return true;
    }

    // A forged index can reset the FMP as well as overwrite the indexed object's header.
    // The next allocation then overlaps the first, even though they have different sites.
    function reset() external pure returns (uint256) {
        uint256[] memory first = new uint256[](1);
        uint256 pointer;
        uint256 index;
        assembly {
            mstore(first, not(0))
            pointer := first
            index := shr(5, sub(0x40, add(first, 32)))
        }
        first[index] = pointer;
        uint256[] memory second = new uint256[](1);
        first[0] = 7;
        second[0] = 9;
        return first[0];
    }
}
