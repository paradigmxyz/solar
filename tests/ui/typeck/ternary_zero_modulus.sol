// solc's constant evaluator does not fold conditionals, so it reports no arithmetic modulo zero
// through them.

contract C {
    uint256 constant ZERO = true ? 0 : 1;

    function add(uint256 a, uint256 b) public pure returns (uint256) {
        return true ? a : addmod(a, b, true ? 0 : 1);
    }

    function mul(uint256 a, uint256 b) public pure returns (uint256) {
        return true ? a : mulmod(a, b, true ? 0 : 1);
    }

    function mulConstant(uint256 a, uint256 b) public pure returns (uint256) {
        return true ? a : mulmod(a, b, ZERO);
    }
}
