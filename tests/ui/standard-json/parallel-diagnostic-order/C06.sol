import "./Base.sol";

contract C06 is Base {
    constructor() public {}

    fallback() external payable {}

    /// @param missing no such parameter
    function f() external {}

    function g() external view returns (uint256) {
        return block.difficulty;
    }

    function h() external pure returns (uint256 x) {
        assembly {
            x := msize()
        }
    }
}
