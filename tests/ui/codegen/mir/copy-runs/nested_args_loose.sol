//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// The second caller does not check `a`, so its base can wrap past the end of calldata and
// the words keep their loads.
// CHECK-LABEL: fn @copy3
// CHECK-NOT: calldatacopy {{v[0-9]+}}, arg0, 96

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
        }
        return copy3(a + 4);
    }
}
