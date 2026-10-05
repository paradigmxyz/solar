//@ codegen-matrix: standard
//@ run-call: deleteReturn => 0
//@ run-call: assignmentReturn => 7

contract UnitReturnValues {
    uint256 private total;

    function deleteReturn() external returns (uint256) {
        total = 7;
        deleteAndReturn();
        return total;
    }

    function deleteAndReturn() internal {
        return delete total;
    }

    function assignmentReturn() external returns (uint256) {
        assignAndReturn();
        return total;
    }

    function assignAndReturn() internal {
        return (total,) = (7, 9);
    }
}
