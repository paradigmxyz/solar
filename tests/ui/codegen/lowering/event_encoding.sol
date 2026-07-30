//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

contract EventEncoding {
    event ArrayEvent(uint256[2] values);
    event AnonymousEvent(address indexed sender, uint256 value) anonymous;
    event IndexedBytes(bytes indexed value);
    event IndexedFixedBytes(bytes3 indexed value);
    event NamedEvent(uint256 a, uint256 b);

    // CHECK-LABEL: fn @emitArray
    // CHECK: log1 0, 64, 0x625921711aa49386aa5b640487cbc3efdcbda2656254ae2c5ad71b2ede1efcf4
    function emitArray(uint256[2] memory values) external {
        emit ArrayEvent(values);
    }

    // CHECK-LABEL: fn @emitAnonymous
    // CHECK: log1 0, 32, arg0
    function emitAnonymous(address sender, uint256 value) external {
        emit AnonymousEvent(sender, value);
    }

    // CHECK-LABEL: fn @emitIndexedBytes
    // CHECK: [[HASH:v[0-9]+]] = keccak256_bytes {{v[0-9]+}}
    // CHECK: log2 0, 0, {{[^,]+}}, [[HASH]]
    function emitIndexedBytes(bytes memory value) external {
        emit IndexedBytes(value);
    }

    // CHECK-LABEL: fn @emitIndexedFixedBytes
    // CHECK: log2 0, 0, {{[^,]+}}, 0x6162630000000000000000000000000000000000000000000000000000000000
    function emitIndexedFixedBytes() external {
        emit IndexedFixedBytes("abc");
    }

    // CHECK-LABEL: fn @emitNamed
    // CHECK: mstore 0, 1
    // CHECK: mstore 32, 2
    // CHECK: log1 0, 64,
    function emitNamed() external {
        emit NamedEvent({b: 2, a: 1});
    }
}
