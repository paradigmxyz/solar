//@compile-flags: -Zdump=evm-ir-runtime
//@ filecheck:

// A calldata dynamic array converted to memory (declaration initializer,
// assignment, or a struct-literal field) must materialize as a
// `[length][elems...]` copy. Lowering the conversion through the generic
// expression path handed out the raw calldata head word as if it were a
// memory pointer, so the copy read length 0 (a silent miscompile — aave's
// flashloan params are built exactly this way). Runtime behavior is verified
// equal to solc 0.8.30 separately, including empty arrays and >32-byte bytes.

contract C {
    struct P {
        uint256 base;
        uint256[] xs;
        bytes tag;
    }

    uint256 public acc;

    // CHECK-LABEL: @module C_runtime
    // CHECK: push 0x2be02e45
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[ASSIGN_BODY:bb[0-9]+]], [[NEXT:bb[0-9]+]]
    // CHECK: [[NEXT]]:
    // CHECK: push 0x3ce9e381
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[STRUCT_BODY:bb[0-9]+]], [[NEXT:bb[0-9]+]]
    // CHECK: [[NEXT]]:
    // CHECK: push 0x7da1365e
    // CHECK-NEXT: eq
    // CHECK-NEXT: jumpi [[ACC_BODY:bb[0-9]+]], [[NEXT:bb[0-9]+]]
    // CHECK: [[NEXT]]:
    // CHECK-NEXT: push 0x874b8e9d
    // CHECK-NEXT: sub
    // CHECK-NEXT: jumpi [[REJECT:bb[0-9]+]], [[DECL_BODY:bb[0-9]+]]
    // CHECK: [[DECL_BODY]]:
    // CHECK: jumpi [[REJECT]], [[DECL_HEAD:bb[0-9]+]]
    // CHECK: [[DECL_HEAD]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK: or
    // CHECK-NEXT: jumpi [[REJECT]], [[DECL_LEN:bb[0-9]+]]
    // CHECK: [[DECL_LEN]]:
    // CHECK: calldataload
    // CHECK: push 5
    // CHECK-NEXT: shr
    // CHECK: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[DECL_SCALE:bb[0-9]+]]
    // CHECK: [[DECL_SCALE]]:
    // CHECK: push 5
    // CHECK-NEXT: shl
    // CHECK: eq
    // CHECK-NEXT: jumpi [[DECL_RANGE:bb[0-9]+]], [[ALLOC_FAIL:bb[0-9]+]]
    // CHECK: [[ALLOC_FAIL]] [cold]:
    // CHECK: push 65
    // CHECK: revert
    // CHECK: [[REJECT]] [cold]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: push 0
    // CHECK-NEXT: revert
    // CHECK: [[ELEMENT:bb[0-9]+]]:
    // CHECK-NEXT: push 32
    // CHECK: mul
    // CHECK: mload
    // CHECK: add
    // CHECK: jumpi [[OVERFLOW:bb[0-9]+]], [[STEP:bb[0-9]+]]
    // CHECK: [[STEP]]:
    // CHECK: push 1
    // CHECK-NEXT: add
    // CHECK: jump [[LOOP:bb[0-9]+]]
    // CHECK: [[LOOP]]:
    // CHECK: mload
    // CHECK: lt
    // CHECK: jumpi [[INDEX_CHECK:bb[0-9]+]], [[STORE_RESULT:bb[0-9]+]]
    // CHECK: [[STORE_RESULT]]:
    // CHECK: push 0
    // CHECK-NEXT: sstore
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[INDEX_CHECK]]:
    // CHECK-NEXT: jumpi [[ELEMENT]], {{bb[0-9]+}}
    // CHECK: [[DECL_RANGE]]:
    // CHECK: calldatasize
    // CHECK: sgt
    // CHECK-NEXT: jumpi [[REJECT]], [[DECL_ALLOC:bb[0-9]+]]
    // CHECK: [[DECL_ALLOC]]:
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: lt
    // CHECK-NEXT: jumpi [[ALLOC_FAIL]], [[DECL_COPY:bb[0-9]+]]
    // CHECK: [[DECL_COPY]]:
    // CHECK: push 64
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: mstore
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: calldatacopy
    // CHECK: jump [[LOOP]]
    function viaDecl(uint256[] calldata xs) external returns (uint256) {
        uint256[] memory m = xs;
        uint256 s = 0;
        for (uint256 i = 0; i < m.length; i++) {
            s += m[i];
        }
        acc = s;
        return s;
    }

    // CHECK-NEXT: [[ASSIGN_RANGE:bb[0-9]+]]:
    // CHECK: calldatasize
    // CHECK: sgt
    // CHECK-NEXT: jumpi [[REJECT]], [[ASSIGN_ALLOC:bb[0-9]+]]
    // CHECK: [[ASSIGN_ALLOC]]:
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: lt
    // CHECK-NEXT: jumpi [[ALLOC_FAIL]], [[ASSIGN_COPY:bb[0-9]+]]
    // CHECK: [[ASSIGN_COPY]]:
    // CHECK: push 64
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: mstore
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: calldatacopy
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    function viaAssign(uint256[] calldata xs) external pure returns (uint256) {
        uint256[] memory m;
        m = xs;
        return m.length;
    }

    // CHECK: push 100
    // CHECK: div
    // CHECK: jumpi {{bb[0-9]+}}, [[OVERFLOW]]
    // CHECK-NEXT: [[STRUCT_RANGE:bb[0-9]+]]:
    // CHECK: calldatasize
    // CHECK: sgt
    // CHECK-NEXT: jumpi [[REJECT]], [[STRUCT_ALLOC:bb[0-9]+]]
    // CHECK: [[STRUCT_ALLOC]]:
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: lt
    // CHECK-NEXT: jumpi [[ALLOC_FAIL]], [[ARRAY_COPY:bb[0-9]+]]
    // CHECK: [[ARRAY_COPY]]:
    // CHECK: push 64
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: mstore
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: calldatacopy
    // CHECK: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[TAG_RANGE:bb[0-9]+]]
    // CHECK: [[TAG_RANGE]]:
    // CHECK: calldatasize
    // CHECK: sgt
    // CHECK-NEXT: jumpi [[REJECT]], [[TAG_COPY:bb[0-9]+]]
    // CHECK: [[TAG_COPY]]:
    // CHECK-NEXT: push 63
    // CHECK: push 31
    // CHECK-NEXT: not
    // CHECK-NEXT: and
    // CHECK: push 64
    // CHECK-NEXT: mload
    // CHECK: push 64
    // CHECK-NEXT: mstore
    // CHECK: mstore
    // CHECK-NEXT: push 32
    // CHECK: add
    // CHECK: calldatacopy
    // CHECK: mload
    // CHECK: mload
    // CHECK-NEXT: mload
    // CHECK-NEXT: push 10
    // CHECK: [[OVERFLOW]] [cold]:
    // CHECK: push 17
    // CHECK: revert
    // CHECK: [[ACC_BODY]]:
    // CHECK-NEXT: push 0
    // CHECK-NEXT: sload
    // CHECK-NEXT: push 0
    // CHECK-NEXT: mstore
    // CHECK-NEXT: push 32
    // CHECK-NEXT: push 0
    // CHECK-NEXT: return
    // CHECK: [[STRUCT_BODY]]:
    // CHECK: jumpi [[REJECT]], [[STRUCT_HEAD:bb[0-9]+]]
    // CHECK: [[STRUCT_HEAD]]:
    // CHECK-NEXT: push 36
    // CHECK-NEXT: calldataload
    // CHECK: or
    // CHECK-NEXT: jumpi [[REJECT]], [[STRUCT_LEN:bb[0-9]+]]
    // CHECK: [[STRUCT_LEN]]:
    // CHECK: calldataload
    // CHECK: push 5
    // CHECK-NEXT: shr
    // CHECK: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[TAG_HEAD:bb[0-9]+]]
    // CHECK: [[TAG_HEAD]]:
    // CHECK-NEXT: push 68
    // CHECK-NEXT: calldataload
    // CHECK: or
    // CHECK: calldataload
    // CHECK: gt
    // CHECK: or
    // CHECK-NEXT: jumpi [[REJECT]], [[STRUCT_SCALE:bb[0-9]+]]
    // CHECK: [[STRUCT_SCALE]]:
    // CHECK: push 5
    // CHECK-NEXT: shl
    // CHECK: eq
    // CHECK-NEXT: jumpi [[STRUCT_RANGE]], [[ALLOC_FAIL]]
    // CHECK: [[ASSIGN_BODY]]:
    // CHECK: jumpi [[REJECT]], [[ASSIGN_HEAD:bb[0-9]+]]
    // CHECK: [[ASSIGN_HEAD]]:
    // CHECK-NEXT: push 4
    // CHECK-NEXT: calldataload
    // CHECK: or
    // CHECK-NEXT: jumpi [[REJECT]], [[ASSIGN_LEN:bb[0-9]+]]
    // CHECK: [[ASSIGN_LEN]]:
    // CHECK: calldataload
    // CHECK: push 5
    // CHECK-NEXT: shr
    // CHECK: gt
    // CHECK-NEXT: jumpi [[REJECT]], [[ASSIGN_SCALE:bb[0-9]+]]
    // CHECK: [[ASSIGN_SCALE]]:
    // CHECK: push 5
    // CHECK-NEXT: shl
    // CHECK: eq
    // CHECK-NEXT: jumpi [[ASSIGN_RANGE]], [[ALLOC_FAIL]]
    function viaStructLiteral(uint256 base, uint256[] calldata xs, bytes calldata tag)
        external
        pure
        returns (uint256)
    {
        P memory p = P({base: base, xs: xs, tag: tag});
        return p.base + p.xs.length * 10 + p.tag.length * 100;
    }
}
