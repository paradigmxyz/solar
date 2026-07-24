//@ check-pass
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
    function init(InitInput calldata input, address sink) external pure returns (uint256) {
        return input.decimals + uint160(sink);
    }

    // A static struct stays inlined in the head, one slot per field.
    function flat(StaticPair calldata pair) external pure returns (uint256) {
        return pair.x;
    }
}

// The dynamic struct occupies ONE head slot (its offset) and `sink` the
// next, so the function takes two head words; the offset is range-checked
// and the struct rebuilds from `4 + offset`.
// DYNSTRUCT-LABEL: fn @init(arg0: u256, arg1: address)
// DYNSTRUCT: gt arg0, 0xffffffffffffffff
// DYNSTRUCT: add 4, arg0
// The static struct stays inlined: one head word per field.
// DYNSTRUCT-LABEL: fn @flat(arg0: u256, arg1: u256)
// DYNSTRUCT: mstore v{{[0-9]+}}, arg0
