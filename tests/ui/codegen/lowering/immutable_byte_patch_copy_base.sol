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
        return 0x5186aa3f6a741c8d3220cb42c34e5e0af989fee640ccdc1b4a3b9a402d234406;
    }

    function b() external pure returns (uint256) {
        return 0x4c21f6d6e07a931e5468ab539814c456e5cc2ec155b1494ae49060cf1d39e90b;
    }

    function c() external pure returns (uint256) {
        return 0xc60587ed52d73c6e856103bd4ea8b91fe4a98d20f5eb58f3b47081ea82beb3bf;
    }

    // CHECK-LABEL: (deployment)
    // CHECK: CODECOPY
    // CHECK: PUSH1 0xb6
    // CHECK-NEXT: DUP1
    // CHECK-NEXT: PUSH1
    // CHECK-NEXT: PUSH0
    // CHECK-NEXT: CODECOPY
    // CHECK-NEXT: PUSH1 0xc0
    // CHECK-NEXT: MLOAD
    // CHECK-NEXT: PUSH1 0xaf
    // CHECK-NEXT: MSTORE8
    function f() external view returns (uint8) {
        return x;
    }
}
