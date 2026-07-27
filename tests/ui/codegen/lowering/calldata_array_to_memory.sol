//@compile-flags: -Zcodegen -Zdump=evm-ir-runtime
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

    // CHECK: push 0x2be02e45
    // CHECK: eq
    // CHECK-NEXT: push [[ASSIGN_BODY:bb[0-9]+]]
    // CHECK: push 0x3ce9e381
    // CHECK: eq
    // CHECK-NEXT: push [[STRUCT_BODY:bb[0-9]+]]
    // CHECK: push 0x7da1365e
    // CHECK: eq
    // CHECK-NEXT: push [[ACC_BODY:bb[0-9]+]]
    // CHECK: push 0x874b8e9d
    // CHECK: eq
    // CHECK-NEXT: push [[DECL_BODY:bb[0-9]+]]
    // CHECK: [[ACC_BODY]]:
    // CHECK: sload
    // CHECK: jump [[RETURN:bb[0-9]+]]
    // CHECK: [[RETURN]]:
    // CHECK: return
    // CHECK: [[DECL_BODY]]:
    // CHECK: calldatacopy
    // CHECK: sstore
    function viaDecl(uint256[] calldata xs) external returns (uint256) {
        uint256[] memory m = xs;
        uint256 s = 0;
        for (uint256 i = 0; i < m.length; i++) {
            s += m[i];
        }
        acc = s;
        return s;
    }

    // CHECK: [[ASSIGN_BODY]]:
    // CHECK: calldatacopy
    // CHECK: mload
    // CHECK: jump [[RETURN]]
    function viaAssign(uint256[] calldata xs) external pure returns (uint256) {
        uint256[] memory m;
        m = xs;
        return m.length;
    }

    // CHECK: [[STRUCT_BODY]]:
    // CHECK: calldatacopy
    // CHECK: calldatacopy
    // CHECK: mload
    // CHECK: mload
    // CHECK: jump [[RETURN]]
    function viaStructLiteral(uint256 base, uint256[] calldata xs, bytes calldata tag)
        external
        pure
        returns (uint256)
    {
        P memory p = P({base: base, xs: xs, tag: tag});
        return p.base + p.xs.length * 10 + p.tag.length * 100;
    }
}
