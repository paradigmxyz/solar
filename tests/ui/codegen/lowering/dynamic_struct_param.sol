//@compile-flags: -Zcodegen -Zdump=mir
//@filecheck: --check-prefix=DYNSTRUCT

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
    // DYNSTRUCT-LABEL: fn @init
    // DYNSTRUCT: gt arg0, 0xffffffffffffffff
    // DYNSTRUCT: add 4, arg0
    // DYNSTRUCT: alloc raw, exact, uninitialized, infallible, 128
    // DYNSTRUCT-COUNT-2: calldatacopy
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    // A static struct stays inlined in the head, one slot per field.
    // DYNSTRUCT-LABEL: fn @flat
    // DYNSTRUCT: mstore v{{[0-9]+}}, arg0
    // DYNSTRUCT: mstore v{{[0-9]+}}, arg1
    function flat(StaticPair calldata pair) external pure returns (uint256) {
        return pair.x;
    }
}
