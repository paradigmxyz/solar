//@ codegen-matrix: standard deploy
//@[deploy] compile-flags: -Ogas -Zdump=disasm-deploy
//@[deploy] filecheck:
//@ run-call: v; constructor=[5] => 15
//@ run-call: a; constructor=[5] => 0x1111111111111111111111111111111111111111
//@ run-call: c; constructor=[5] => 0x1111111111111111111111111111111111111111

// Creation code runs once and pays calldata for every byte, so gas builds weigh the
// bytes of the constructor against its execution gas at 16 to 1. The checked `p * 3`
// divides to test for overflow instead of comparing against a 32-byte limit, and the
// address that three stores share stays on the stack after one push.
// CHECK-LABEL: // === {{.*}}:Constants (deployment) ===
// CHECK-NOT: PUSH32
// CHECK: DIV
// CHECK: PUSH20 0x1111111111111111111111111111111111111111
// CHECK-NOT: PUSH20
// CHECK: RETURN
contract Constants {
    uint256 public v;
    address public a;
    address public b;
    address public c;

    constructor(uint256 p) {
        v = p * 3;
        a = address(0x1111111111111111111111111111111111111111);
        b = address(0x1111111111111111111111111111111111111111);
        c = address(0x1111111111111111111111111111111111111111);
    }
}
