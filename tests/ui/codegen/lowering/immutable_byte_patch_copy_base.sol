//@ compile-flags: -Zdump=disasm-deploy
//@ filecheck:

// The runtime code ends just below the constructor's immutable staging area
// at 0xc0. A one-byte immutable patch writes only the byte after its `PUSH1`,
// so the patch at 0xb5 stays below that area and the runtime copy can start at
// address zero. Treating the patch as a full word would reach past 0xc0 and
// move the copy above the staging area.
contract ImmutableBytePatchCopyBase {
    uint8 immutable x;

    constructor(uint8 v) {
        x = v;
    }

    function a() external pure returns (uint256) {
        return 0x1111111111111111111111111111111111111111111111111111111111111111;
    }

    function b() external pure returns (uint256) {
        return 0x2222222222222222222222222222222222222222222222222222222222222222;
    }

    function c() external pure returns (uint256) {
        return 0x3333333333333333333333333333333333333333333333333333333333333333;
    }

    // CHECK-LABEL: (deployment)
    // CHECK: CODECOPY
    // CHECK: PUSH1 0xbe
    // CHECK-NEXT: DUP1
    // CHECK-NEXT: PUSH1
    // CHECK-NEXT: PUSH0
    // CHECK-NEXT: CODECOPY
    // CHECK-NEXT: PUSH1 0xc0
    // CHECK-NEXT: MLOAD
    // CHECK-NEXT: PUSH1 0xb5
    // CHECK-NEXT: MSTORE8
    function f() external view returns (uint8) {
        return x;
    }
}
