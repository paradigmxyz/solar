//@ codegen-matrix: standard
//@ run-call: allocate 1 => 1
//@ run-call: slot 0 => 0x0000000000000000000000000000000000000000000000000000000000000080
//@ run-call: slot 1 => 9
//@ run-call-fail: allocate 0 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000473746f7000000000000000000000000000000000000000000000000000000000

// The `Error(string)` revert overwrites the free-memory-pointer word, but only in its own block.
// The allocation on the other path, and a return of the slot after a store in a different block,
// still need the pointer initialized at entry.
contract RevertStringFmpOtherBlock {
    function allocate(uint256 x) external pure returns (uint256) {
        require(x != 0, "stop");
        bytes memory data = new bytes(1);
        return data.length;
    }

    function slot(uint256 x) external pure returns (uint256) {
        assembly {
            if x { mstore(0x40, 9) }
            return(0x40, 32)
        }
    }
}
