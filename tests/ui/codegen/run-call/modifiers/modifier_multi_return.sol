//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f false => 1
//@ run-call: f true => 1
// Solc's via-IR modifier frames reset return variables per placeholder.

contract ModifierMultiReturn {
    modifier repeat(bool twice) {
        if (twice) _;
        _;
    }

    function f(bool twice) external pure repeat(twice) returns (uint256 r) {
        r += 1;
        return r;
    }
}
