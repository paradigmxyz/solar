//@ codegen-matrix: standard ir
//@[ir] filecheck:
//@[ir] compile-flags: -Ogas -Zdump=evm-ir
//@ run-call: ping; constructor=[false, 9] => 7
//@ run-call-fail: run 9; constructor=[false, 9] => 0x0000000000000000000000000000000000000000000000000000000000000009

// CHECK-LABEL: @module TerminalTailEntryCreation_deployment
// CHECK: {{push 160[[:space:]]+mload[[:space:]]+push 64[[:space:]]+add[[:space:]]+mload}}
// CHECK: revert
// CHECK-LABEL: @module TerminalTailEntryCreation_runtime
// CHECK-NOT: mload
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: jump [[BODY:bb[0-9]+]]
// CHECK: [[BODY]] [cold]:
// CHECK-NEXT: push 0
// CHECK-NEXT: mstore
// CHECK-NEXT: push 32
// CHECK-NEXT: push 0
// CHECK-NEXT: revert
// CHECK: push 4
// CHECK-NEXT: calldataload
// CHECK-NEXT: jump [[BODY]]

contract TerminalTailEntryCreation {
    constructor(bool reject, uint256 a) { if (reject) fail(a); }
    function ping() external pure returns(uint256) { return 7; }
    function run(uint256 a) external pure { fail(a); }
    function other(uint256 a) external pure { fail(a); }
    function fail(uint256 a) internal pure {
        assembly { mstore(0, a) revert(0, 32) }
    }
}
