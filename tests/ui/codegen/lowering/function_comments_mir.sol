//@ compile-flags: -Zdump=mir -Zmir-pipeline=none

// MIR prints a `;` note before each function with its source signature and role, and block labels
// list their predecessors. The note and the lists are comments that the MIR parser skips.

contract C {
    uint256 public total;

    constructor(uint256 start) {
        total = start;
    }

    function pick(bool left, uint256 a, uint256 b) external pure returns (uint256) {
        if (left) return a;
        return step(b);
    }

    function step(uint256 x) internal pure returns (uint256) {
        return x + 1;
    }

    receive() external payable {}

    fallback() external {}
}
