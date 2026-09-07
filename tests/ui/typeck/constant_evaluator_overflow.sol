// ported-from: test/libsolidity/syntaxTests/constantEvaluator/overflow.sol
// ported-from: test/libsolidity/syntaxTests/constantEvaluator/underflow.sol
// ported-from: test/libsolidity/syntaxTests/constantEvaluator/underflow_unary.sol
// ported-from: test/libsolidity/syntaxTests/constantEvaluator/unary_fine.sol
// ported-from: test/libsolidity/syntaxTests/storageLayoutSpecifier/constant_divided_by_its_negation.sol

contract Overflow {
    uint8 constant a = 255;
    uint16 constant b = a + 2;
    function f() public pure {
        uint[b] memory x; //~ ERROR: failed to evaluate constant: arithmetic overflow
    }
}

contract Underflow {
    uint8 constant a = 0;
    function f() public pure {
        uint[a - 1] memory x; //~ ERROR: failed to evaluate constant: arithmetic overflow
    }
}

contract UnderflowUnary {
    int8 constant a = -128;
    function f() public pure {
        uint[-a] memory x; //~ ERROR: failed to evaluate constant: arithmetic overflow
    }
}

contract UnaryFine {
    int8 constant a = -7;
    function f() public pure {
        uint[-a] memory x;
        x[0] = 2;
    }
}

uint constant n = 100;

contract ConstantDividedByItsNegation layout at n / ~n {} //~ ERROR: failed to evaluate constant: arithmetic overflow
