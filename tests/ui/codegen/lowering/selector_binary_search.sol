//@ revisions: gas size runtime
//@[gas] compile-flags: -O gas -Zdump=evm-ir-runtime
//@[gas] filecheck: --check-prefix=GAS
//@[size] compile-flags: -O size -Zdump=evm-ir-runtime
//@[size] filecheck: --check-prefix=SIZE
//@[runtime] compile-flags: -O gas
//@[runtime] run-call: f0() => 0
//@[runtime] run-call: f1() => 1
//@[runtime] run-call: f2() => 2
//@[runtime] run-call: f3() => 3
//@[runtime] run-call: f4() => 4
//@[runtime] run-call: f5() => 5
//@[runtime] run-call: f6() => 6
//@[runtime] run-call: f7() => 7
//@[runtime] run-call: f8() => 8
//@[runtime] run-call-fail: 0xffffffff

// Large gas-optimized selector switches use binary split nodes before their equality leaves. Size
// optimization retains the linear chain because each split duplicates the default edge.
//
// GAS-LABEL: @module runtime
// GAS-COUNT-1: gt
// SIZE-LABEL: @module runtime
// SIZE-NOT: gt
contract SelectorBinarySearch {
    function f0() external pure returns (uint256) { return 0; }
    function f1() external pure returns (uint256) { return 1; }
    function f2() external pure returns (uint256) { return 2; }
    function f3() external pure returns (uint256) { return 3; }
    function f4() external pure returns (uint256) { return 4; }
    function f5() external pure returns (uint256) { return 5; }
    function f6() external pure returns (uint256) { return 6; }
    function f7() external pure returns (uint256) { return 7; }
    function f8() external pure returns (uint256) { return 8; }
}
