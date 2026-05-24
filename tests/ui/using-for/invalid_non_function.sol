//@compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/using/using_non_function.sol

uint256 constant X = 1;

contract C {
    using {X} for uint256; //~ ERROR: expected function name
}
