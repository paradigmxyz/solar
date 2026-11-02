//@ revisions: mir run
//@[mir] compile-flags: -Zmir-pipeline=lower-arithmetic -Zdump=mir
//@[mir] filecheck:
//@[run] compile-flags: -Zmir-pipeline=lower-checks,lower-builtins,lower-arithmetic,lower-abi,lower-abi-encode,lower-dispatch,lower-structs,lower-frame-slots,lower-mapping-slots,lower-memory-objects,lower-slices,lower-alloc,lower-memory-zero,lower-integers,lower-evm-shaped
//@[run] normalize-stdout-test: "[0-9a-f]{40,}" -> "BYTECODE"
//@[run] run-call: add8 1, 2 => 3
//@[run] run-call: add8 255, 0 => 255
//@[run] run-call-fail: add8 255, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[run] run-call: sub 5, 7 => -2
//@[run] run-call-fail: sub -57896044618658097711785492504343953926634992332820282019728792003956564819968, 1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[run] run-call: div -7, 2 => -3
//@[run] run-call-fail: div 1, 0 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@[run] run-call-fail: div -57896044618658097711785492504343953926634992332820282019728792003956564819968, -1 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[run] run-call: powConst 5 => 243
//@[run] run-call: powConst 161 => 65542350158517637872691969508970705427701150314738255642438471845988797065603
//@[run] run-call-fail: powConst 162 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[run] run-call: powVar 2, 255 => 57896044618658097711785492504343953926634992332820282019728792003956564819968
//@[run] run-call-fail: powVar 2, 256 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[run] run-call: powNeg 3 => -27
//@[run] run-call: mul64 4294967295, 4294967297 => 18446744073709551615
//@[run] run-call-fail: mul64 4294967296, 4294967296 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@[run] run-call: wrap 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 1 => 0
// Checked operations become scalar arithmetic followed by a panic branch on the overflow
// condition. Unchecked arithmetic has nothing to expand.

contract CheckedOps {
    // A narrow sum only compares the widened result with the type's maximum.
    // CHECK-LABEL: fn @add8(
    // CHECK: [[SUM:v[0-9]+]] = add {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: [[OVER:v[0-9]+]] = gt [[SUM]], 255
    // CHECK-NEXT: jumpi [[OVER]]
    // CHECK-NOT: checked_add
    function add8(uint8 a, uint8 b) public pure returns (uint8) {
        return a + b;
    }

    // A signed difference overflows when the operand signs differ and the result's sign
    // differs from the minuend's.
    // CHECK-LABEL: fn @sub(
    // CHECK: [[DIFF:v[0-9]+]] = sub arg0, arg1
    // CHECK: slt [[DIFF]], 0
    // CHECK: xor
    // CHECK-NOT: checked_sub
    function sub(int256 a, int256 b) public pure returns (int256) {
        return a - b;
    }

    // Division checks the divisor, and a signed one also checks `min / -1`.
    // CHECK-LABEL: fn @div(
    // CHECK: ne arg1, 0
    // CHECK: eq arg0, 0x8000000000000000000000000000000000000000000000000000000000000000
    // CHECK: sdiv arg0, arg1
    // CHECK-NOT: checked_div
    function div(int256 a, int256 b) public pure returns (int256) {
        return a / b;
    }

    // A constant base needs no loop: one comparison against the largest exponent that fits,
    // then one `exp`.
    // CHECK-LABEL: fn @powConst(
    // CHECK: gt arg0, 161
    // CHECK: exp 3, arg0
    // CHECK-NOT: phi
    function powConst(uint256 e) public pure returns (uint256) {
        return 3 ** e;
    }

    // A variable base squares in a loop and checks every product.
    // CHECK-LABEL: fn @powVar(
    // CHECK: phi
    // CHECK: shr
    // CHECK-NOT: exp
    function powVar(uint256 b, uint256 e) public pure returns (uint256) {
        return b ** e;
    }

    // A negative constant base keeps the loop, since its powers alternate in sign.
    // CHECK-LABEL: fn @powNeg(
    // CHECK: phi [bb0: 1]
    // CHECK: sdiv
    // CHECK-NOT: exp
    function powNeg(uint256 e) public pure returns (int256) {
        int256 b = -3;
        return b ** e;
    }

    // Operands of at most 128 bits cannot wrap a word, so only the range is checked.
    // CHECK-LABEL: fn @mul64(
    // CHECK: [[PRODUCT:v[0-9]+]] = mul {{v[0-9]+}}, {{v[0-9]+}}
    // CHECK-NEXT: gt [[PRODUCT]], 0xffffffffffffffff
    // CHECK-NOT: div
    function mul64(uint64 a, uint64 b) public pure returns (uint64) {
        return a * b;
    }

    // CHECK-LABEL: fn @wrap(
    // CHECK-NEXT: bb0:
    // CHECK-NEXT: [[SUM:v[0-9]+]] = add arg0, arg1
    // CHECK-NEXT: ret [[SUM]]
    function wrap(uint256 a, uint256 b) public pure returns (uint256) {
        unchecked {
            return a + b;
        }
    }
}
