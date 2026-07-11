// ported-from: test/libsolidity/syntaxTests/using/global_for_type_defined_elsewhere.sol

library L {
    struct Inner {
        uint256 x;
    }
}

function idInner(L.Inner memory) pure {}

using {idInner} for L.Inner global; //~ ERROR: can only use `global` with types defined in the same source unit at file level
