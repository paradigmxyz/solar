//@compile-flags: -O none -Zdump=mir
//@filecheck:
// ported-from: test/libsolidity/semanticTests/cleanup/indexed_log_topic_during_explicit_downcast_during_emissions.sol

contract EventEncoding {
    event ArrayEvent(uint256[2] values);
    event AnonymousEvent(address indexed sender, uint256 value) anonymous;
    event IndexedBytes(bytes indexed value);
    event IndexedFixedBytes(bytes3 indexed value);
    event IndexedDirtyFixedBytes(bytes1 indexed value);
    event IndexedExternalFunction(function() external indexed target);
    event NamedEvent(uint256 a, uint256 b);

    // CHECK-LABEL: fn @emitArray
    // CHECK: [[ARRAY_ENCODED:v[0-9]+]] = abi_encode [array<2, word>], scratch, args
    // CHECK: [[ARRAY_PTR:v[0-9]+]] = extract_value slice<memory>, [[ARRAY_ENCODED]], 0
    // CHECK: [[ARRAY_LEN:v[0-9]+]] = extract_value slice<memory>, [[ARRAY_ENCODED]], 1
    // CHECK: log [[ARRAY_PTR]], [[ARRAY_LEN]], 0x625921711aa49386aa5b640487cbc3efdcbda2656254ae2c5ad71b2ede1efcf4
    function emitArray(uint256[2] memory values) external {
        emit ArrayEvent(values);
    }

    // CHECK-LABEL: fn @emitAnonymous
    // CHECK: mstore 0, arg1
    // CHECK: [[WORD:v[0-9]+]] = zext {{i(1|160)}} arg0
    // CHECK: log 0, 32, [[WORD]]
    function emitAnonymous(address sender, uint256 value) external {
        emit AnonymousEvent(sender, value);
    }

    // CHECK-LABEL: fn @emitIndexedBytes
    // CHECK: [[DATA:v[0-9]+]] = ptradd arg0, 32
    // CHECK-NEXT: [[LEN:v[0-9]+]] = mload arg0
    // CHECK-NEXT: [[HASH:v[0-9]+]] = keccak256 [[DATA]], [[LEN]]
    // CHECK: log 0, 0, {{[^,]+}}, [[HASH]]
    function emitIndexedBytes(bytes memory value) external {
        emit IndexedBytes(value);
    }

    // CHECK-LABEL: fn @emitIndexedFixedBytes
    // CHECK: log 0, 0, {{[^,]+}}, 0x6162630000000000000000000000000000000000000000000000000000000000
    function emitIndexedFixedBytes() external {
        emit IndexedFixedBytes("abc");
    }

    // CHECK-LABEL: fn @emitIndexedDirtyFixedBytes
    // CHECK: log 0, 0, {{[^,]+}}, 0x3100000000000000000000000000000000000000000000000000000000000000
    function emitIndexedDirtyFixedBytes() external {
        bytes1 value;
        assembly {
            value := 0x3131313131313131313131313131313131313131313131313131313131313131
        }
        emit IndexedDirtyFixedBytes(value);
    }

    // CHECK-LABEL: fn @emitIndexedExternalFunction
    // CHECK: [[POINTER:v[0-9]+]] = or
    // CHECK: [[SHIFTED:v[0-9]+]] = shl [[POINTER]], 64
    // CHECK: log 0, 0, {{[^,]+}}, [[SHIFTED]]
    function emitIndexedExternalFunction() external {
        emit IndexedExternalFunction(this.target);
    }

    function target() external {}

    // CHECK-LABEL: fn @emitNamed
    // CHECK: mstore 0, 1
    // CHECK-NEXT: mstore 32, 2
    // CHECK-NEXT: log 0, 64,
    function emitNamed() external {
        emit NamedEvent({b: 2, a: 1});
    }
}
