// ported-from: test/libsolidity/syntaxTests/literalOperations/exponent_fine.sol
// ported-from: test/libsolidity/syntaxTests/types/rational_number_exp_limit_fine.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/constant_initialized_with_unlimited_arithmetic_expression.sol

contract ExponentFine {
    function f() public pure {
        uint a;
        a = a ** 1E5;
        a = 0 ** 1E1233;
    }
}

contract RationalNumberExpLimitFine {
    function f() public pure {
        int a;
        a = 0 ** 1E1233;
        a = 1 ** 1E1233;
        a = -1 ** 1E1233;
        a = 0E123456789;
    }
}

uint constant CONST1 = 1.23e100 / 2e50;
contract C layout at CONST1 {}
uint constant CONST2 = 2**256 * (500e-3);
contract D layout at CONST2 {}
uint constant CONST3 = (2**255 * 2) - (2**256 + 1) + 1;
contract E layout at CONST3 {}
