//@ revisions: opt run
//@[opt] compile-flags: -Ogas -Zdump=mir
//@[opt] filecheck:
//@[run] compile-flags: -Ogas
//@ run-call: copied [0x0000000000000000000000000000000000000001, 0xffffffffffffffffffffffffffffffffffffffff] => [0x0000000000000000000000000000000000000001, 0xffffffffffffffffffffffffffffffffffffffff]
//@ run-call: copied [] => []

// The only wrapper returning `address[]` returns what an internal copy built
// from validated input. Element cleanup proves the copy's words fit an
// address, so the result is encoded with one copy even though no second
// wrapper of the same return type shares a cleanup helper with it.
contract SingleCallArray {
    function copy(address[] memory a) internal pure returns (address[] memory result) {
        result = new address[](a.length);
        for (uint256 i; i < a.length; ++i) {
            result[i] = a[i];
        }
    }

    // CHECK-LABEL: fn @copied{{[( ]}}
    // CHECK: icall @copy
    // CHECK-NOT: and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: mcopy
    // CHECK: returndata
    function copied(address[] memory a) external pure returns (address[] memory) {
        return copy(a);
    }
}
