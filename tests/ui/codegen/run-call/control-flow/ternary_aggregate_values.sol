//@ codegen-matrix: standard
//@ run-call: choose true, 7, 11 => 7, 11
//@ run-call: choose false, 7, 11 => 11, 7
//@ run-call: memoryChoice true, 0 => 7, 0, 1
//@ run-call: memoryChoice false, 0 => 0, 7, 1
//@ run-call-fail: memoryChoice true, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000032

contract TernaryAggregateValues {
    function choose(bool flag, uint256 a, uint256 b) external pure returns (uint256, uint256) {
        return flag ? (a, b) : (b, a);
    }

    function memoryChoice(bool flag, uint256 index) external pure returns (uint256, uint256, uint256) {
        uint256[] memory left = new uint256[](1);
        uint256[] memory right = new uint256[](1);
        uint256 evaluations;
        (++evaluations == 1 && flag ? left : right)[index] += 7;
        return (left[0], right[0], evaluations);
    }
}
