//@ revisions: none gas size
//@[none] compile-flags: -O none -Zevm-ir-pipeline=none -Zdump=evm-ir
//@[none] filecheck:
//@[gas] compile-flags: -O gas
//@[size] compile-flags: -O size
//@ run-call: read; constructor=[7, -123, 66051, 456] => 7, -123, 66051, 456
//@ run-call: reverse; constructor=[7, -123, 66051, 456] => 456, 66051, -123, 7
//@ run-call: adjust 1; constructor=[7, -123, 66051, 456] => 8, -124, 66050, 457
//@ run-call: read; constructor=[255, -32768, 16777215, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff] => 255, -32768, 16777215, 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff

// Repeated patches cover full words, narrow signed values, left-aligned bytes, and MSTORE8.
// Narrow MSTORE windows can overlap neighboring placeholders, whose bytes must stay intact when
// patches are interleaved by immutable rather than ordered by their deployed runtime position.
contract ImmutablePatchOrder {
    uint8 private immutable a;
    int16 private immutable b;
    bytes3 private immutable c;
    uint256 private immutable d;

    // CHECK-LABEL: @module ImmutablePatchOrder_deployment
    // CHECK: codecopy
    // CHECK: codecopy
    // CHECK-NEXT: push
    // CHECK-NEXT: mload
    // CHECK-NEXT: push
    // CHECK-NEXT: mstore8
    // CHECK-NEXT: push
    // CHECK-NEXT: mload
    constructor(uint8 a_, int16 b_, uint24 c_, uint256 d_) {
        a = a_;
        b = b_;
        c = bytes3(c_);
        d = d_;
    }

    function read() external view returns (uint8, int16, uint24, uint256) {
        return (a, b, uint24(c), d);
    }

    function reverse() external view returns (uint256, uint24, int16, uint8) {
        return (d, uint24(c), b, a);
    }

    function adjust(uint8 x) external view returns (uint256, int256, uint24, uint256) {
        return (uint256(a) + x, int256(b) - int256(uint256(x)), uint24(c) ^ x, d + x);
    }
}
