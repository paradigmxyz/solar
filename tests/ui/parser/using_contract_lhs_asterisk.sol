// Solc test: test/libsolidity/syntaxTests/using/using_lhs_asterisk_contract.sol.

contract C {
    using * for uint256; //~ ERROR: expected identifier
}
