//@compile-flags: -O none -Zdump=mir
//@filecheck:

// Functions returning a memory reference can now recurse: the return is a
// 32-byte pointer that flows through the internal-frame slots, and callee heap
// allocations persist (the free pointer is not restored across a call). The
// public function is lowered both as its external ABI entry and an internal
// copy; the internal copy returns the struct pointer (it must NOT expand to
// fields like the external entry). A public function returning a dynamic array
// of word elements is passed to the lower-abi phase as a semantic return value.
// Runtime-verified against solc: build(3) == (6,3); squares(4) == [0,1,4,9];
// mkArr(3) == [0,10,20].
contract C {
    struct P {
        uint256 x;
        uint256 y;
    }

    // recursive function returning a memory struct
    // CHECK-LABEL: fn @build(
    // CHECK: internal_call @build, 1,
    // CHECK: alloc memorystruct<2>
    // CHECK: memory_object_store_field memorystruct<2>
    // CHECK: ret
    function build(uint256 n) public pure returns (P memory) {
        if (n == 0) return P({x: 0, y: 0});
        P memory inner = build(n - 1);
        return P({x: inner.x + n, y: inner.y + 1});
    }

    // public function returning a dynamic word-array (external ABI encoding)
    // CHECK-LABEL: fn @mkArr{{[( ]}}
    // CHECK: alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: set_memory_object_len memoryarray, {{v[0-9]+}}, arg0
    // CHECK: ret
    function mkArr(uint256 n) public pure returns (uint256[] memory) {
        uint256[] memory r = new uint256[](n);
        for (uint256 i = 0; i < n; i++) r[i] = i * 10;
        return r;
    }

    // recursive helper returning a memory array, consumed by a public function
    // CHECK-LABEL: fn @fillImpl{{[( ]}}
    // CHECK: memory_object_store_element memoryarray<1>, arg0, arg1
    // CHECK: [[NEXT:v[0-9]+]] = add arg1, 1
    // CHECK: [[RESULT:v[0-9]+]] = internal_call @fillImpl, 1, arg0, [[NEXT]]
    // CHECK: ret [[RESULT]]
    // CHECK: ret arg0
    function fillImpl(uint256[] memory a, uint256 i) internal pure returns (uint256[] memory) {
        if (i == a.length) return a;
        a[i] = i * i;
        return fillImpl(a, i + 1);
    }

    // CHECK-LABEL: fn @squares{{[( ]}}
    // CHECK: alloc memoryarray<1>, exact, zeroed, panic
    // CHECK: internal_call @fillImpl, 1, {{v[0-9]+}}, 0
    // CHECK: ret
    function squares(uint256 n) public pure returns (uint256[] memory) {
        return fillImpl(new uint256[](n), 0);
    }
}
