library Lib {
    function bump(uint256 value) public view returns (uint256) {
        return value * block.timestamp;
    }
}
