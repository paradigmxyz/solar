//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_wrong_count.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_invalid_bytes.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_invalid_hex_literal.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_invalid_number_literal.sol

bytes constant BYTES_ARG = "abcdef";

contract WrongCount {
    uint x = erc7201(); //~ ERROR: wrong argument count
    uint y = erc7201("12", "34"); //~ ERROR: wrong argument count
    uint z = erc7201("A", "BC", "D"); //~ ERROR: wrong argument count
}

contract InvalidArgumentTypes {
    function invalidBytes() public pure returns (uint) {
        return erc7201(BYTES_ARG);
        //~^ ERROR: mismatched types
    }

    function invalidHexLiteral() public pure returns (uint) {
        return erc7201(hex"001122FF");
        //~^ ERROR: mismatched types
    }

    function invalidNumberLiteral() public pure returns (uint) {
        return erc7201(123);
        //~^ ERROR: mismatched types
    }
}
