//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f [7] => 7
//@ run-call: f [1, 2] => 1

contract ModifierMemoryArgument {
    modifier rewrite(uint256[] memory values) {
        values[0] = 9;
        _;
    }

    function f(uint256[] calldata values) external pure rewrite(values) returns (uint256) {
        return values[0];
    }
}
