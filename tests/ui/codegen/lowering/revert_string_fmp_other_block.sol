//@ codegen-matrix: standard
//@ run-call: allocate 1 => 1
//@ run-call-fail: allocate 0 => 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000473746f7000000000000000000000000000000000000000000000000000000000

// The `Error(string)` revert overwrites the free-memory-pointer word, but only in its own block.
// The allocation on the other path still needs the pointer initialized at entry.
contract RevertStringFmpOtherBlock {
    function allocate(uint256 x) external pure returns (uint256) {
        require(x != 0, "stop");
        bytes memory data = new bytes(1);
        return data.length;
    }
}
