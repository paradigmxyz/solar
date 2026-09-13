//@ compile-flags: -Zmir-pipeline=element-cleanup -Zpass-diff
//@ filecheck:

// Element reads of address arrays are masked to the type; the masks go
// where every word the array can hold is canonical.
contract ElementCleanup {
    // ABI decoding validates every element of an external array.
    // CHECK-LABEL: {{^[ +-].*}}fn @first
    // CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function first(address[] memory a) external pure returns (address) {
        return a[0];
    }

    // A fresh array is zeroed and only ever stores canonical words.
    // CHECK-LABEL: {{^[ +-].*}}fn @fresh
    // CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    // CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function fresh(uint256 n) external pure returns (address) {
        address[] memory a = new address[](n);
        a[0] = a[1];
        return a[2];
    }

    // A helper whose every call site, including its own, passes a canonical
    // array reads without masks.
    // CHECK-LABEL: {{^[ +-].*}}fn @last
    // CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function last(address[] memory a, uint256 i) internal pure returns (address) {
        if (i + 1 == a.length) return a[i];
        return last(a, i + 1);
    }

    // CHECK-LABEL: {{^[ +-].*}}fn @tail
    function tail(address[] memory a) external pure returns (address) {
        return last(a, 0);
    }

    // An assembly store may leave a dirty word: the read keeps its mask.
    // CHECK-LABEL: {{^[ +-].*}}fn @dirty
    // CHECK-NOT: {{^-}}
    function dirty(address[] memory a) external pure returns (address) {
        assembly {
            mstore(add(a, 0x20), 0x0123456789abcdef0123456789abcdef0123456789abcdef)
        }
        return a[0];
    }

    // A nominal parameter is cleaned before it is stored, so the helper's
    // caller still reads without a mask after the call.
    // CHECK-LABEL: {{^[ +-].*}}fn @fill
    // CHECK: {{^ +}}memory_object_store_element
    function fill(address[] memory a, address x) internal pure {
        a[0] = x;
    }

    // CHECK-LABEL: {{^[ +-].*}}fn @filled
    // CHECK: - {{v[0-9]+}} = and {{v[0-9]+}}, 0xffffffffffffffffffffffffffffffffffffffff
    function filled(address[] memory a, address x) external pure returns (address) {
        fill(a, x);
        return a[0];
    }

    // A mask narrower than the words a `uint256` array holds is a real mask.
    // CHECK-LABEL: {{^[ +-].*}}fn @narrowed
    // CHECK-NOT: {{^-}}
    function narrowed(uint256[] memory a, uint256 x) external pure returns (uint256) {
        a[0] = x;
        return a[1] & type(uint160).max;
    }
}
