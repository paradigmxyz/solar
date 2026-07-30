library Lib {
    function inc(uint256 value) public pure returns (uint256) {
        return value + 1;
    }
}

contract C {
    function inc(uint256 value) external pure returns (uint256) {
        return Lib.inc(value);
    }
}
