//@ compile-flags: -Ogas
//@ run-call: first 12, 7, 5 => 16
//@ run-call: second 12, 7, 5 => 16

contract ResidentStaticArgsMulDiv {
    function first(uint256 x, uint256 y, uint256 d) external pure returns (uint256) {
        return mulDivDown(x, y, d);
    }

    function second(uint256 x, uint256 y, uint256 d) external pure returns (uint256) {
        return mulDivDown(x, y, d);
    }

    // Keep checked multiplication's original operands resident through its
    // overflow branch so the following division can still consume `d`.
    function mulDivDown(uint256 x, uint256 y, uint256 d) internal pure returns (uint256) {
        return (x * y) / d;
    }
}
