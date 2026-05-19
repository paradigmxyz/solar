//@compile-flags: -Ztypeck
// Ported from test/libsolidity/syntaxTests/lvalues/library_mapping.sol.

contract L {
    function f(mapping(uint=>uint) storage x, mapping(uint=>uint) storage y) internal {
        x = y; //~ ERROR: types in storage containing (nested) mappings cannot be assigned to
    }
}
