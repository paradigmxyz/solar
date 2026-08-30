// ported-from: test/libsolidity/syntaxTests/denominations/invalid_denomination_no_whitespace.sol

contract C {
    uint constant y = 1wei; //~ ERROR: identifier-start is not allowed at end of a number
}
