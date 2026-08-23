//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: andSkipsRhs() => 0, false
//@ run-call: orSkipsRhs() => 0, true
//@ run-call: andRunsRhs() => 1, true
//@ run-call: orRunsRhs() => 1, true

contract LogicalShortCircuit {
    uint256 calls;

    function bump() external returns (bool) {
        calls++;
        return true;
    }

    function andSkipsRhs() external returns (uint256, bool) {
        bool result = false && this.bump();
        return (calls, result);
    }

    function orSkipsRhs() external returns (uint256, bool) {
        bool result = true || this.bump();
        return (calls, result);
    }

    function andRunsRhs() external returns (uint256, bool) {
        bool result = true && this.bump();
        return (calls, result);
    }

    function orRunsRhs() external returns (uint256, bool) {
        bool result = false || this.bump();
        return (calls, result);
    }
}
