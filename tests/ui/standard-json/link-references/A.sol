library Lib {
    function bump(uint256 value) public view returns (uint256) {
        return value + block.timestamp;
    }
}

contract Links {
    uint256 public immutable initial;

    constructor() {
        initial = Lib.bump(1);
    }

    function bump(uint256 value) external returns (uint256) {
        return Lib.bump(value);
    }
}
