//@compile-flags: -Zdump=mir
//@filecheck: --check-prefix=UDO

// User-defined operators on a value type (`using {add as +, neg as -} for T`)
// lower to a call to the operator function; the UDVT operands are transparent
// words at runtime. Verified behaviorally against solc for binary, unary, and
// chained operators.

type BalanceDelta is int256;
using {add as +, sub as -, neg as -, flip as ~} for BalanceDelta global;

function add(BalanceDelta a, BalanceDelta b) pure returns (BalanceDelta) {
    return BalanceDelta.wrap(BalanceDelta.unwrap(a) + BalanceDelta.unwrap(b));
}
function sub(BalanceDelta a, BalanceDelta b) pure returns (BalanceDelta) {
    return BalanceDelta.wrap(BalanceDelta.unwrap(a) - BalanceDelta.unwrap(b));
}
function neg(BalanceDelta a) pure returns (BalanceDelta) {
    return BalanceDelta.wrap(-BalanceDelta.unwrap(a));
}
function flip(BalanceDelta) pure returns (BalanceDelta) {
    return BalanceDelta.wrap(7);
}

contract UserDefinedOperators {
    // UDO-LABEL: fn @doAdd
    // UDO: icall
    function doAdd(int256 x, int256 y) public pure returns (int256) {
        return BalanceDelta.unwrap(BalanceDelta.wrap(x) + BalanceDelta.wrap(y));
    }

    // UDO-LABEL: fn @doNeg
    // UDO: icall
    function doNeg(int256 x) public pure returns (int256) {
        return BalanceDelta.unwrap(-BalanceDelta.wrap(x));
    }

    // UDO-LABEL: fn @doFlip
    // The constant user-defined operator folds before code generation.
    // UDO: mstore 128, 7
    // UDO: returndata 128, 32
    function doFlip(int256 x) public pure returns (int256) {
        return BalanceDelta.unwrap(~BalanceDelta.wrap(x));
    }
}
