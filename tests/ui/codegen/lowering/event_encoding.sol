//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract EventEncoding {
    event ArrayEvent(uint256[2] values);
    event AnonymousEvent(address indexed sender, uint256 value) anonymous;
    event IndexedBytes(bytes indexed value);
    event IndexedFixedBytes(bytes3 indexed value);
    event IndexedExternalFunction(function() external indexed target);
    event NamedEvent(uint256 a, uint256 b);

    // CHECK-LABEL: fn @emitArray
    // CHECK: [[ARRAY_ENCODED:v[0-9]+]] = abi_encode [array<2, word>], args
    // CHECK: [[ARRAY_PTR:v[0-9]+]] = slice_ptr [[ARRAY_ENCODED]]
    // CHECK: [[ARRAY_LEN:v[0-9]+]] = slice_len [[ARRAY_ENCODED]]
    // CHECK: log1 [[ARRAY_PTR]], [[ARRAY_LEN]], 0x625921711aa49386aa5b640487cbc3efdcbda2656254ae2c5ad71b2ede1efcf4
    function emitArray(uint256[2] memory values) external {
        emit ArrayEvent(values);
    }

    // CHECK-LABEL: fn @emitAnonymous
    // CHECK: [[ANON_ENCODED:v[0-9]+]] = abi_encode [word], args
    // CHECK: [[ANON_PTR:v[0-9]+]] = slice_ptr [[ANON_ENCODED]]
    // CHECK: [[ANON_LEN:v[0-9]+]] = slice_len [[ANON_ENCODED]]
    // CHECK: log1 [[ANON_PTR]], [[ANON_LEN]], arg0
    function emitAnonymous(address sender, uint256 value) external {
        emit AnonymousEvent(sender, value);
    }

    // CHECK-LABEL: fn @emitIndexedBytes
    // CHECK: [[HASH:v[0-9]+]] = keccak256_bytes arg0
    // CHECK: log2 0, 0, {{[^,]+}}, [[HASH]]
    function emitIndexedBytes(bytes memory value) external {
        emit IndexedBytes(value);
    }

    // CHECK-LABEL: fn @emitIndexedFixedBytes
    // CHECK: log2 0, 0, {{[^,]+}}, 0x6162630000000000000000000000000000000000000000000000000000000000
    function emitIndexedFixedBytes() external {
        emit IndexedFixedBytes("abc");
    }

    // CHECK-LABEL: fn @emitIndexedExternalFunction
    // CHECK: [[POINTER:v[0-9]+]] = or
    // CHECK: [[SHIFTED:v[0-9]+]] = shl 64, [[POINTER]]
    // CHECK: log2 0, 0, {{[^,]+}}, [[SHIFTED]]
    function emitIndexedExternalFunction() external {
        emit IndexedExternalFunction(this.target);
    }

    function target() external {}

    // CHECK-LABEL: fn @emitNamed
    // CHECK: [[NAMED_ENCODED:v[0-9]+]] = abi_encode [word, word], args 1, 2
    // CHECK: [[NAMED_PTR:v[0-9]+]] = slice_ptr [[NAMED_ENCODED]]
    // CHECK: [[NAMED_LEN:v[0-9]+]] = slice_len [[NAMED_ENCODED]]
    // CHECK: log1 [[NAMED_PTR]], [[NAMED_LEN]],
    function emitNamed() external {
        emit NamedEvent({b: 2, a: 1});
    }
}
