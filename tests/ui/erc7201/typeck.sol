//@ compile-flags: -Ztypeck
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_string_variable.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_wrong_count.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_invalid_bytes.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_invalid_hex_literal.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_invalid_number_literal.sol
// ported-from: test/libsolidity/syntaxTests/nameAndTypeResolution/erc7201_builtin_const_var_assignment.sol
// ported-from: test/libsolidity/syntaxTests/constantEvaluator/erc7201_builtin.sol
// ported-from: test/libsolidity/syntaxTests/globalFunctions/erc7201_builtin_param_string_variable_comptime.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/erc7201_builtin_param_string_literal_comptime.sol

string constant FILE_LEVEL_STR = "test.file";
bytes constant BYTES_ARG = "abcdef";

contract LayoutAtErc7201 layout at erc7201("storageBase") {}

contract ConstVarAssignment {
    uint constant x = erc7201("abc");

    function f() public pure returns (uint) {
        return mulmod(x, 10, 37);
    }
}

contract ConstantEvaluator {
    uint constant x = erc7201("A");
    uint[x] array;
}

contract Erc7201Valid {
    string constant STATE_VAR_STR = "example.contract";

    bytes32 internal constant STATE_POSITION =
        bytes32(erc7201("example.Erc7201Builtin.State"));

    uint constant A_SLOT = erc7201("A");
    uint8[A_SLOT] arrayFromConstant;
    uint8[erc7201(FILE_LEVEL_STR)] arrayFromFileConstant;

    uint8[
        3082882499010855372434788006333239417747066088816838060270105547210302925568
    ] expectedZeroLastByteArray;
    uint8[erc7201("85")] zeroLastByteArray;

    function fileLevel() public pure returns (uint) {
        return erc7201(FILE_LEVEL_STR);
    }

    function stateVar() public pure returns (uint) {
        return erc7201(STATE_VAR_STR);
    }

    function localVar() public pure returns (uint) {
        string memory localVarStr = "example.main";
        return erc7201(localVarStr);
    }

    function funcMemParam(string memory paramStr) public pure returns (uint) {
        return erc7201(paramStr);
    }

    function funcCallDataParam(string calldata paramStr) public pure returns (uint) {
        return erc7201(paramStr);
    }

    function zeroLastByteArrayMatchesExpected() public view {
        uint8[
            3082882499010855372434788006333239417747066088816838060270105547210302925568
        ] storage ref = zeroLastByteArray;
        ref;
    }
}

contract Erc7201Invalid {
    uint x = erc7201(); //~ ERROR: wrong argument count
    uint y = erc7201("12", "34"); //~ ERROR: wrong argument count
    uint z = erc7201("A", "BC", "D"); //~ ERROR: wrong argument count

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
