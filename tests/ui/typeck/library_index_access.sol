// ported-from: test/libsolidity/syntaxTests/array/invalid/library_index_access.sol

library C {
    function f() view public {
        C[0]; //~ ERROR: index access for library types is not possible
    }
}
