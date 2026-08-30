library Lib {
    function bump(uint256 value) public view returns (uint256) {
        return value + block.timestamp;
    }
}

contract Child {
    function bump(uint256 value) external returns (uint256) {
        return Lib.bump(value);
    }
}

contract Parent {
    function create() external returns (address) {
        return address(new Child());
    }

    function code() external pure returns (bytes memory) {
        return type(Child).creationCode;
    }
}
