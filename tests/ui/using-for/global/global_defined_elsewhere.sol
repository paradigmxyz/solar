//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/global_for_type_defined_elsewhere.sol

library L {
    struct Inner {
        uint256 x;
    }
}

function id256(uint256 x) pure returns (uint256) {
    return x;
}

using {id256} for L.Inner global; //~ ERROR: can only use `global` with types defined in the same source unit at file level
//~^ ERROR: cannot be attached
