//@ codegen-matrix: standard
//@[gas] compile-flags: -Zdump=mir
//@[gas] filecheck: --check-prefix=UDO
//@ run-call: doAdd 2, 3 => 5
//@ run-call: doNeg 2 => -2
//@ run-call: doFlip 42 => 7

// User-defined operators on a value type (`using {add as +, neg as -} for T`)
// lower through the operator functions and can inline their bodies. The UDVT
// operands are transparent words at runtime.

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
    // UDO: add
    function doAdd(int256 x, int256 y) public pure returns (int256) {
        return BalanceDelta.unwrap(BalanceDelta.wrap(x) + BalanceDelta.wrap(y));
    }

    // UDO-LABEL: fn @doNeg
    // UDO: sub 0,
    function doNeg(int256 x) public pure returns (int256) {
        return BalanceDelta.unwrap(-BalanceDelta.wrap(x));
    }

    // UDO-LABEL: fn @doFlip
    // The constant user-defined operator folds before code generation.
    // UDO: mstore 0, 7
    // UDO: returndata 0, 32
    function doFlip(int256 x) public pure returns (int256) {
        return BalanceDelta.unwrap(~BalanceDelta.wrap(x));
    }
}
