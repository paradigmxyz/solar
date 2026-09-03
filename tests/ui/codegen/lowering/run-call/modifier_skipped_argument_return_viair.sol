//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f => 0
// Solc's via-IR modifier frames discard argument-side return writes when the
// placeholder is skipped.

contract ModifierSkippedArgumentReturnViaIr {
    modifier skip(uint256 value) {
        if (value == 7) return;
        _;
    }

    function f() external pure skip(r = 7) returns (uint256 r) {}
}
