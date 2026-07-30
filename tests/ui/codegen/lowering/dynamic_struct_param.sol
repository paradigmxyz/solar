//@compile-flags: -Zcodegen -O none -Zdump=mir
//@filecheck:

struct InitInput {
    address asset;
    uint8 decimals;
    string name;
    bytes params;
}

struct StaticPair {
    uint256 x;
    address who;
}

contract DynamicStructParam {
    // A struct with dynamic members is dynamically encoded: one head slot
    // holds its offset from the args start, and the fields — including
    // nested dynamic offsets relative to the struct's own base — rebuild
    // recursively from the tail.
    // The dynamic struct occupies one head slot and `sink` the next.
    // CHECK-LABEL: fn @init{{[( ]}}
    // CHECK: gt arg0, 0xffffffffffffffff
    // CHECK: add 4, arg0
    // CHECK: alloc raw, exact, uninitialized, infallible, 160
    // CHECK-COUNT-2: calldatacopy
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    // A static struct stays inlined in the head, one slot per field.
    // CHECK-LABEL: fn @flat{{[( ]}}
    // CHECK: mstore v{{[0-9]+}}, arg0
    // CHECK: mstore v{{[0-9]+}}, arg1
    function flat(StaticPair calldata pair) external pure returns (uint256) {
        return pair.x;
    }
}
