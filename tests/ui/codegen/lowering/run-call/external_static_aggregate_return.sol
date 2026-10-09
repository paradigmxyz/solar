//@ codegen-matrix: standard
//@[mir] filecheck:
//@ run-call: ExternalStaticAggregateReturn::structReturn 7 => 15
//@ run-call: ExternalStaticAggregateReturn::arrayReturn 7 => 24

// An external call that returns one static aggregate decodes it in place from
// the bytes object it returns into.
contract ExternalStaticAggregateReturn {
    struct S {
        uint256 a;
        uint256 b;
    }

    function makeStruct(uint256 x) external pure returns (S memory) {
        return S(x, x + 1);
    }

    function makeArray(uint256 x) external pure returns (uint256[3] memory) {
        return [x, x + 1, x + 2];
    }

    // CHECK-LABEL: fn @structReturn{{[( ]}}
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 96
    // CHECK-NOT: memory_object_copy_from_slice
    // CHECK: abi_decode [tuple<u256, u256>], [[BUF]]
    function structReturn(uint256 x) external view returns (uint256) {
        S memory s = this.makeStruct(x);
        return s.a + s.b;
    }

    // CHECK-LABEL: fn @arrayReturn{{[( ]}}
    // CHECK: [[BUF:v[0-9]+]] = alloc memorybytes, exact, uninitialized, infallible, 128
    // CHECK-NOT: memory_object_copy_from_slice
    // CHECK: abi_decode [array<3, u256>], [[BUF]]
    function arrayReturn(uint256 x) external view returns (uint256) {
        uint256[3] memory a = this.makeArray(x);
        return a[0] + a[1] + a[2];
    }
}
