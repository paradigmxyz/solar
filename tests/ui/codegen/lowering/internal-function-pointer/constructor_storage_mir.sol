//@ compile-flags: -O none -Zdump=mir
//@ filecheck:

// ported-from: test/libsolidity/semanticTests/constructor/store_internal_unused_function_in_constructor.sol

// The 8-byte function pointer packs into slot 0: the constructor
// read-modify-writes its bytes and readers mask them back out.
// CHECK-LABEL: fn @constructor(
// CHECK: or {{v[0-9]+}}, [[ONLY_STORED:[0-9]+]]
// CHECK: sstore 0, {{v[0-9]+}}
contract ConstructorStoredFunctionPointer {
    function() internal returns (uint256) storedOnly;

    constructor() {
        storedOnly = onlyStored;
    }

    function onlyStored() internal pure returns (uint256) {
        return 7;
    }

    // CHECK-LABEL: fn @callStoredOnly(
    // CHECK: [[WORD:v[0-9]+]] = sload 0
    // CHECK: [[NARROW:v[0-9]+]] = trunc i256 [[WORD]] to i64
    // CHECK: [[STORED_ONLY:v[0-9]+]] = zext i64 [[NARROW]] to i256
    // CHECK: icall @internal_dispatcher_p_none_r_uint256, [[STORED_ONLY]]
    // CHECK-LABEL: fn @internal_dispatcher_p_none_r_uint256(
    // CHECK: eq arg0, [[ONLY_STORED]]
    // CHECK: icall @onlyStored
    function callStoredOnly() public returns (uint256) {
        return storedOnly();
    }
}
