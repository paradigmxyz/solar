//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// Both callers keep `a` at most `calldatasize - 100`, with `calldatasize` at least 100, so the
// base they pass, `a + 4`, is inside calldata. The callee checks the room behind it, and the
// three words are one copy.
// CHECK-LABEL: fn @copy3
// CHECK: calldatacopy {{v[0-9]+}}, arg0, 96

contract Test {
    // Three words at `base`, after a check of the room behind it.
    function copy3(uint256 base) internal pure returns (bytes memory out) {
        out = new bytes(96);
        assembly {
            if lt(sub(calldatasize(), base), 96) { revert(0, 0) }
            let q := add(out, 32)
            mstore(q, calldataload(base))
            mstore(add(q, 32), calldataload(add(base, 32)))
            mstore(add(q, 64), calldataload(add(base, 64)))
        }
    }

    function p1() external pure returns (bytes memory) {
        uint256 a;
        assembly {
            if lt(calldatasize(), 100) { revert(0, 0) }
            a := calldataload(4)
            if gt(a, sub(calldatasize(), 100)) { revert(0, 0) }
        }
        return copy3(a + 4);
    }

    function p2() external pure returns (bytes memory) {
        uint256 a;
        assembly {
            if lt(calldatasize(), 100) { revert(0, 0) }
            a := calldataload(36)
            if gt(a, sub(calldatasize(), 100)) { revert(0, 0) }
        }
        return copy3(a + 4);
    }
}
