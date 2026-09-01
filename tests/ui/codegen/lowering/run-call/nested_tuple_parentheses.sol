//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f0 => 2, true
//@ run-call: f1 => 1
//@ run-call: f2 => 2
//@ run-call: f3 => 3
//@ run-call: f4 => 4
//@ run-call: rhsParens => 7, 8
//@ run-call: scalarParens => 7
// ported-from: test/libsolidity/semanticTests/types/nested_tuples.sol

contract NestedTupleParentheses {
    function f0() external pure returns (int256, bool) {
        int256 a;
        bool b;
        ((a, b)) = (2, true);
        return (a, b);
    }

    function f1() external pure returns (int256) {
        int256 a;
        (((a,),)) = ((1, 2), 3);
        return a;
    }

    function f2() external pure returns (int256) {
        int256 a;
        (((,a),)) = ((1, 2), 3);
        return a;
    }

    function f3() external pure returns (int256) {
        int256 a = 3;
        ((,),) = ((7, 8), 9);
        return a;
    }

    function f4() external pure returns (int256) {
        int256 a;
        (a,) = (4, (8, 16, 32));
        return a;
    }

    function rhsParens() external pure returns (uint256, uint256) {
        uint256 a;
        uint256 b;
        (a, b) = (((7, 8)));
        return (a, b);
    }

    function scalarParens() external pure returns (uint256) {
        uint256 a;
        ((a)) = 7;
        return a;
    }
}
