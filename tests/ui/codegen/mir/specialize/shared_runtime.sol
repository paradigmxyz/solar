//@ codegen-matrix: standard
//@ run-call: first 4 => 11
//@ run-call: second 8 => 15
//@ run-call: third 9 => 16
//@ run-call: generic true, 3 => 81
//@ run-call: generic false, 3 => 10
contract SharedConstants {
    function first(uint256 x) external pure returns (uint256) { return helper(false, x); }
    function second(uint256 x) external pure returns (uint256) { return helper(false, x); }
    function third(uint256 x) external pure returns (uint256) { return helper(false, x); }
    function generic(bool mode, uint256 x) external pure returns (uint256) { return helper(mode, x); }
    function helper(bool mode, uint256 x) internal pure returns (uint256) {
        if (mode) return x * x * x * x;
        return x + 7;
    }
}
