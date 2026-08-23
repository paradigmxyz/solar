//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f() => 7

contract ModifierNamedArguments {
    modifier check(uint256 expected) {
        require(expected == 7);
        _;
    }

    function f() external pure check({expected: 7}) returns (uint256) {
        return 7;
    }
}
