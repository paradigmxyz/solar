//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/using_non_function.sol

contract C {
    function() internal pure x;

    using {x} for uint256; //~ ERROR: expected function name
}
