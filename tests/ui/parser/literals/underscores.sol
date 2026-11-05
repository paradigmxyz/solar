// ported-from: test/libsolidity/syntaxTests/parsing/lexer_numbers_with_underscores_decimal_fail.sol
// ported-from: test/libsolidity/syntaxTests/parsing/lexer_numbers_with_underscores_fixed_fail.sol
// ported-from: test/libsolidity/syntaxTests/parsing/lexer_numbers_with_underscores_hex_fail.sol

contract C {
    function f() public pure {
        uint D1 = 1234_; //~ ERROR: invalid use of underscores in number literal
        uint D2 = 12__34; //~ ERROR: invalid use of underscores in number literal
        uint D3 = 12_e34; //~ ERROR: invalid use of underscores in number literal
        uint D4 = 12e_34; //~ ERROR: invalid use of underscores in number literal

        uint F1 = 3.1415_; //~ ERROR: invalid use of underscores in number literal
        uint F2 = 3__1.4__15; //~ ERROR: invalid use of underscores in number literal
        uint F3 = 1_.2; //~ ERROR: invalid use of underscores in number literal
        uint F4 = 1._2; //~ ERROR: invalid use of underscores in number literal
        uint F5 = 1.2e_12; //~ ERROR: invalid use of underscores in number literal
        uint F6 = 1._; //~ ERROR: invalid use of underscores in number literal

        uint X1 = 0x1234__1234__1234__123; //~ ERROR: invalid use of underscores in number literal
        uint X2 = 0x1_; //~ ERROR: invalid use of underscores in number literal
        uint X3 = 0x1__0; //~ ERROR: invalid use of underscores in number literal

        // A valid underscore between digits, and next to an uppercase exponent as in solc.
        uint V1 = 1_000;
        uint V2 = 0x1_0;
        uint V3 = 1_E5;
        uint V4 = 1E_5;
    }
}
