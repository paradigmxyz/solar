import "./debug_output_helper.sol";

contract C is Helper {
    function f(uint256 x) external pure returns (uint256) {
        return plusOne(x);
    }
}
