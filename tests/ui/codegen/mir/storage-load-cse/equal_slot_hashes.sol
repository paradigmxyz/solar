//@ codegen-matrix: standard dump
//@[dump] compile-flags: -Ogas -Zdump=mir
//@[dump] filecheck:
//@ run-call: quotient 17, 5 => 5
//@ run-call: quotientBy 17, 3, 9 => 9
//@ run-call-fail: quotientBy 17, 0, 9 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000012
//@ run-call: successor 4, 6 => 6
//@ run-call-fail: successor 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff, 6 => 0x4e487b710000000000000000000000000000000000000000000000000000000000000011
//@ run-call: lane 9, 0x100000005 => 5
//@ run-call: lane 7, 0x123456789 => 0x23456789

// Each function reads the slot it has just written through a key that checked
// arithmetic computes twice. Lowering makes the two keys one value, but the
// two mapping slots stay apart until a later CSE. Their hashes of equal
// operands name one slot, so the read becomes the stored word, as it does for a
// key computed with `>>`. The checks themselves still fail where they should.
contract EqualSlotHashes {
    mapping(uint256 => uint256) m;

    // CHECK-LABEL: fn @quotient()
    // CHECK: sstore {{v[0-9]+}}, arg1
    // CHECK-NOT: sload
    // CHECK: returndata
    function quotient(uint256 i, uint256 x) external returns (uint256) {
        m[i / 8] = x;
        return m[i / 8];
    }

    // CHECK-LABEL: fn @quotientBy()
    // CHECK: sstore {{v[0-9]+}}, arg2
    // CHECK-NOT: sload
    // CHECK: returndata
    function quotientBy(uint256 i, uint256 d, uint256 x) external returns (uint256) {
        m[i / d] = x;
        return m[i / d];
    }

    // CHECK-LABEL: fn @successor()
    // CHECK: sstore {{v[0-9]+}}, arg1
    // CHECK-NOT: sload
    // CHECK: returndata
    function successor(uint256 i, uint256 x) external returns (uint256) {
        m[i + 1] = x;
        return m[i + 1];
    }

    // A packed field update followed by a read of the field, as in a bit map.
    // CHECK-LABEL: fn @lane()
    // CHECK: = sload [[SLOT:v[0-9]+]]
    // CHECK: sstore [[SLOT]], [[STORED:v[0-9]+]]
    // CHECK-NOT: sload
    // CHECK: = shr [[STORED]],
    // CHECK: returndata
    function lane(uint256 i, uint256 v) external returns (uint256) {
        uint256 o = (i % 8) * 32;
        uint256 w = m[i / 8];
        m[i / 8] = w ^ ((((w >> o) ^ v) & 0xffffffff) << o);
        return (m[i / 8] >> ((i % 8) * 32)) & 0xffffffff;
    }
}
