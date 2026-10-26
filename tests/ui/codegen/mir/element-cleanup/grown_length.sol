//@ codegen-matrix: standard opt
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck:
//@[opt] normalize-stdout-test: "(?s).+" -> ""
//@ run-call: test 0 => false
//@ run-call: test 1 => true

// A length that assembly grows past the allocation lets the bounds check pass
// for words the width bounds never saw, here the dirty words a caller left past
// the free memory pointer. The element read keeps its mask, as solc's does.
contract GrownLength {
    function dirty() internal pure {
        assembly {
            let p := mload(0x40)
            mstore(add(p, 0x20), not(0))
            mstore(add(p, 0x40), or(shl(160, 1), 0x1234))
        }
    }

    // CHECK-LABEL: fn @find(
    // CHECK: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function find(uint256 n, uint256 i, address x) internal pure returns (bool) {
        address[] memory a = new address[](n);
        assembly {
            mstore(a, 3)
        }
        return a[i] == x;
    }

    function test(uint256 i) external pure returns (bool) {
        dirty();
        return find(0, i, address(0x1234));
    }

    // More calls keep `find` out of line, so its own bounds decide its masks.
    function other(uint256 n) external pure returns (bool) {
        return find(n, 0, address(0)) || find(n, 1, address(1));
    }
}
