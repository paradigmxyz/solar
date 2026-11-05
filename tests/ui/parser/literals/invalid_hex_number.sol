// ported-from: test/libsolidity/syntaxTests/literals/invalid_hex_number.sol

contract C {
    uint x = 0x1000abcdefgh; //~ ERROR: identifier-start is not allowed at end of a number
}
