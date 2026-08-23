//@ filecheck:
// CHECK: @module
//@ codegen-matrix: standard
//@ run-call: f() => 0
// Solc's via-IR modifier frame semantics preserve the incoming return value
// when a modifier skips its placeholder.

contract ModifierSkippedPlaceholderViaIr {
    modifier skip() {
        if (true) return;
        _;
    }

    function f() external pure skip returns (uint256 r) {
        r = 7;
    }
}
