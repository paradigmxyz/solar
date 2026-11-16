// ported-from: test/libsolidity/syntaxTests/using/using_empty_list_err.sol

contract C {
    using {} for uint256; //~ ERROR: expected identifier
}
