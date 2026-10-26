//@ revisions: size gas
//@[size] compile-flags: -Osize -Zdump=mir
//@[gas] compile-flags: -Ogas -Zdump=mir
//@ filecheck:

// The dispatcher checks the head size once for every function, so the decoders below start with
// a known `calldatasize`. Each decoder reads the offset of its struct from calldata, checked
// against the room behind the pointer it adds to, and the checks of its callers keep that pointer
// inside calldata. The words of the nested struct, and the elements of the array, are one copy.
// CHECK-LABEL: fn @decode_calldata_type{{[.0-9]*}}(arg0: i256, arg1: i256)
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 128
// CHECK: calldatacopy {{v[0-9]+}}, {{v[0-9]+}}, 96

contract Test {
    struct Inner { uint256 a; uint256 b; uint256 c; }
    struct Leaf { uint256 p; uint256 q; uint256 r; uint256 s; }
    struct Mid { uint256 k; Leaf leaf; Inner[] arr; }
    struct Top { Mid[] mids; Mid one; uint256 z; }

    function f1(Top memory t) public pure returns (uint256) {
        return t.mids.length + t.one.arr.length + t.z;
    }

    function f2(Top memory t) public pure returns (bytes memory) { return abi.encode(t); }

    function f3(Top memory t) public pure returns (Top memory) { return t; }
}
