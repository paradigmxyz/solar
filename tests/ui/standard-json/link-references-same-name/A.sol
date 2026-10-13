import * as B from "B.sol";

library Lib {
    function bump(uint256 value) public view returns (uint256) {
        return value + block.timestamp;
    }
}

contract Links {
    function bump(uint256 value) external returns (uint256) {
        return Lib.bump(value) + B.Lib.bump(value);
    }
}
