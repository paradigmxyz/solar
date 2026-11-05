// solc's constant evaluator does not fold conditionals, so it reports no division or modulo by
// zero through them.

contract C {
    uint256 constant ZERO = true ? 0 : 1;
    uint256 constant SEVEN = true ? 7 : 1;

    function div() public pure returns (uint256) {
        return true ? 7 : 1 / (true ? 0 : 1);
    }

    function rem() public pure returns (uint256) {
        return true ? 7 : 1 % (true ? 0 : 1);
    }

    function divConstant() public pure returns (uint256) {
        return true ? 7 : 1 / ZERO;
    }

    function lhsConstant(uint256 x) public pure returns (uint256) {
        return x == 0 ? 7 : SEVEN / 0;
    }
}
