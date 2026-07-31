contract Opcodes {
    function difficulty() external view returns (uint256) {
        return block.difficulty;
    }
}
