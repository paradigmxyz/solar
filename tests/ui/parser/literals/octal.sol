// ported-from: test/libsolidity/syntaxTests/literals/invalid_octal_digits.sol
// ported-from: test/libsolidity/syntaxTests/literals/invalid_octal_number.sol
// ported-from: test/libsolidity/syntaxTests/literals/invalid_octal_denomination_no_whitespace.sol

contract C {
    uint a = 098; //~ ERROR: octal numbers not allowed
    uint b = 0100; //~ ERROR: octal numbers not allowed
    uint c = 01gwei; //~ ERROR: octal numbers not allowed
    uint d = 00.5; //~ ERROR: octal numbers not allowed
    uint e = 01e1; //~ ERROR: octal numbers not allowed
}
