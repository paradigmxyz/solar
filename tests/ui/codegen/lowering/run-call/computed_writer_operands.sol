//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: ComputedOperandOrder::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 192, 192 => 3735928559, 4004
//@ run-call: ComputedOperandOrder::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 193, 193 => 3735928559, 4004
//@ run-call: ComputedOperandOrder::run [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18], 4096, 4096 => 3735928559, 4004

// For inputs 1..18, SUB contributes -1, AND 0, OR 260, ADD 261, and XOR 3484.
// CHECK-LABEL: @module ComputedOperandOrder
// CHECK: fn @run(arg0: calldataslice, arg1: u256, arg2: u256)
// CHECK: [[PTR:v[0-9]+]] = slice_ptr arg0
// CHECK-NEXT: [[OFFSET:v[0-9]+]] = add [[PTR]], 0
// CHECK-NEXT: [[FIRST:v[0-9]+]] = calldataload [[OFFSET]]
// CHECK-NEXT: {{v[0-9]+}} = sub [[FIRST]], 256
// CHECK: [[NEXT:v[0-9]+]] = add {{v[0-9]+}}, 32
// CHECK-NEXT: [[SECOND:v[0-9]+]] = calldataload [[NEXT]]
// CHECK-NEXT: {{v[0-9]+}} = sub 256, [[SECOND]]
// CHECK: [[ANDREAD:v[0-9]+]] = calldataload {{v[0-9]+}}
// CHECK-NEXT: {{v[0-9]+}} = and [[ANDREAD]], 256
// CHECK: [[ORREAD:v[0-9]+]] = calldataload {{v[0-9]+}}
// CHECK-NEXT: {{v[0-9]+}} = or [[ORREAD]], 256
// CHECK: [[ADDREAD:v[0-9]+]] = calldataload {{v[0-9]+}}
// CHECK-NEXT: {{v[0-9]+}} = add [[ADDREAD]], 256
// CHECK: [[XORREAD:v[0-9]+]] = calldataload {{v[0-9]+}}
// CHECK-NEXT: {{v[0-9]+}} = xor [[XORREAD]], 256
// CHECK-NOT: @module
// CHECK: mstore arg1, 0xdeadbeef
// CHECK-NOT: @module
// CHECK: {{v[0-9]+}} = mload arg2
contract ComputedOperandOrder {
    function run(uint256[18] calldata values, uint256 destination, uint256 source)
        external pure returns (uint256 observed, uint256 checksum)
    {
        assembly {
            let a00 := sub(calldataload(add(values, 0)), 256)
            let a01 := sub(256, calldataload(add(values, 32)))
            let a02 := and(calldataload(add(values, 64)), 256)
            let a03 := or(calldataload(add(values, 96)), 256)
            let a04 := add(calldataload(add(values, 128)), 256)
            let a05 := xor(calldataload(add(values, 160)), 256)
            let a06 := xor(calldataload(add(values, 192)), 256)
            let a07 := xor(calldataload(add(values, 224)), 256)
            let a08 := xor(calldataload(add(values, 256)), 256)
            let a09 := xor(calldataload(add(values, 288)), 256)
            let a10 := xor(calldataload(add(values, 320)), 256)
            let a11 := xor(calldataload(add(values, 352)), 256)
            let a12 := xor(calldataload(add(values, 384)), 256)
            let a13 := xor(calldataload(add(values, 416)), 256)
            let a14 := xor(calldataload(add(values, 448)), 256)
            let a15 := xor(calldataload(add(values, 480)), 256)
            let a16 := xor(calldataload(add(values, 512)), 256)
            let a17 := xor(calldataload(add(values, 544)), 256)
            mstore(destination, 0xdeadbeef)
            observed := mload(source)
            checksum := add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(add(a00, a01), a02), a03), a04), a05), a06), a07), a08), a09), a10), a11), a12), a13), a14), a15), a16), a17)
        }
    }
}
