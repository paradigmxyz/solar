//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// The source of each iteration is `base + 64 i`, or a cursor stepping by 64. With `base` checked
// below 2^64 no iteration's address wraps, as the loop runs at most 2^64 times, so the two words
// of an iteration are one copy. Without the check the address can wrap to the start of calldata.
// CHECK-LABEL: fn @indexBounded
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 64
// CHECK-LABEL: fn @indexLoose
// CHECK-NOT: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 64
// CHECK-LABEL: fn @cursorBounded
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 64
// CHECK-LABEL: fn @cursorLoose
// CHECK-NOT: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 64

contract Test {
    // A bounded source steps by 64 each iteration; two words copy to a destination stepping by 96.
    function indexBounded(uint256 base, uint256 n) external pure returns (bytes memory out) {
        out = new bytes(n * 96);
        assembly {
            if gt(base, 0xffffffffffffffff) { revert(0, 0) }
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                let s := add(base, mul(i, 64))
                mstore(add(add(out, 32), mul(i, 96)), calldataload(s))
                mstore(add(add(out, 64), mul(i, 96)), calldataload(add(s, 32)))
            }
        }
    }
    function indexLoose(uint256 base, uint256 n) external pure returns (bytes memory out) {
        out = new bytes(n * 96);
        assembly {
            for { let i := 0 } lt(i, n) { i := add(i, 1) } {
                let s := add(base, mul(i, 64))
                mstore(add(add(out, 32), mul(i, 96)), calldataload(s))
                mstore(add(add(out, 64), mul(i, 96)), calldataload(add(s, 32)))
            }
        }
    }
    function cursorBounded(uint256 base, uint256 n) external pure returns (bytes memory out) {
        out = new bytes(n * 96);
        assembly {
            if gt(base, 0xffffffffffffffff) { revert(0, 0) }
            let q := add(out, 32)
            let end := add(q, mul(n, 96))
            for { let p := base } lt(q, end) { p := add(p, 64) q := add(q, 96) } {
                mstore(q, calldataload(p))
                mstore(add(q, 32), calldataload(add(p, 32)))
            }
        }
    }
    function cursorLoose(uint256 base, uint256 n) external pure returns (bytes memory out) {
        out = new bytes(n * 96);
        assembly {
            let q := add(out, 32)
            let end := add(q, mul(n, 96))
            for { let p := base } lt(q, end) { p := add(p, 64) q := add(q, 96) } {
                mstore(q, calldataload(p))
                mstore(add(q, 32), calldataload(add(p, 32)))
            }
        }
    }
}
