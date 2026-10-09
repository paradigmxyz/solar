//@ codegen-matrix: standard
//@ run-call: aligned => 64
//@ run-call: constantValue => 6
//@ run-call: widened => 600
//@ run-call: castBranch => 1
//@ run-call-fail: overflow() => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011

// A ternary has the common mobile type of its branches, so literal branches are computed in an
// integer type rather than exactly.
contract ConstantTernaryLiteral {
    bool constant DEBUG = false;
    uint256 constant D = (true ? 7 : 8) / 3 * 3;

    function aligned() external pure returns (uint256) {
        uint256 value = (DEBUG ? 33 : 65) / 32 * 32;
        return value;
    }

    function constantValue() external pure returns (uint256) {
        return D;
    }

    function widened() external pure returns (uint256) {
        uint256 value = (true ? 2 : 300) * 300;
        return value;
    }

    // A branch that is not a constant leaves the literal's own type.
    function castBranch() external pure returns (uint256) {
        uint256[true ? 1 : uint256(2)] memory values;
        return values.length;
    }

    function overflow() external pure returns (uint256) {
        uint256 value = (true ? 200 : 1) * 2;
        return value;
    }
}
