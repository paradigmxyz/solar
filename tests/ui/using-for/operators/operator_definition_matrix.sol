// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_no_parameters_binary.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_two_parameters_unary.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_and_returning_types_not_matching_using_for.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_taking_or_returning_different_types.sol
// ported-from: test/libsolidity/syntaxTests/operators/userDefined/operator_returning_wrong_types.sol

type Param is uint256;
type Ret is uint256;
type Cmp is uint256;

function p_zero() pure returns (Param) {
    return Param.wrap(0);
}

function p_one(Param a) pure returns (Param) {
    //~^ ERROR: wrong parameters
    return a;
}

function p_three(Param a, Param b, Param c) pure returns (Param) {
    //~^ ERROR: wrong parameters
    b; c;
    return a;
}

function p_first(uint256 a, Param b) pure returns (Param) {
    //~^ ERROR: wrong parameters
    a;
    return b;
}

function p_second(Param a, uint256 b) pure returns (Param) {
    //~^ ERROR: wrong parameters
    b;
    return a;
}

function p_bitnot_two(Param a, Param b) pure returns (Param) {
    //~^ ERROR: wrong parameters
    b;
    return a;
}

using {
    p_zero as +, //~ ERROR: does not have any parameters
    p_one as *,
    p_three as -,
    p_first as /,
    p_second as %,
    p_bitnot_two as ~
} for Param global;

function r_raw(Ret a, Ret b) pure returns (uint256) {
    //~^ ERROR: wrong return parameters
    b;
    return Ret.unwrap(a);
}

function r_none(Ret a, Ret b) pure {
    //~^ ERROR: wrong return parameters
    a; b;
}

function r_two(Ret a, Ret b) pure returns (Ret, Ret) {
    //~^ ERROR: wrong return parameters
    return (a, b);
}

function r_unary_raw(Ret a) pure returns (uint256) {
    //~^ ERROR: wrong return parameters
    return Ret.unwrap(a);
}

function r_minus_two(Ret a) pure returns (Ret, Ret) {
    //~^ ERROR: wrong return parameters
    return (a, a);
}

using {
    r_raw as +,
    r_none as *,
    r_two as /,
    r_unary_raw as ~,
    r_minus_two as -
} for Ret global;

function c_ret_udvt(Cmp a, Cmp b) pure returns (Cmp) {
    //~^ ERROR: wrong return parameters
    b;
    return a;
}

function c_none(Cmp a, Cmp b) pure {
    //~^ ERROR: wrong return parameters
    a; b;
}

function c_two(Cmp a, Cmp b) pure returns (bool, Cmp) {
    //~^ ERROR: wrong return parameters
    b;
    return (true, a);
}

function c_params(Cmp a) pure returns (bool) {
    //~^ ERROR: wrong parameters
    a;
    return true;
}

using {
    c_ret_udvt as <,
    c_none as >,
    c_two as ==,
    c_params as !=
} for Cmp global;

contract C {
    function badParamUses(Param a, Param b) public pure {
        a + b; //~ ERROR: cannot apply builtin operator
        a * b; //~ ERROR: cannot apply builtin operator
        a - b; //~ ERROR: cannot apply builtin operator
        a / b; //~ ERROR: cannot apply builtin operator
        a % b; //~ ERROR: cannot apply builtin operator
        ~a; //~ ERROR: cannot apply unary operator
    }

    function badReturnUses(Ret a, Ret b) public pure {
        a + b;
        a * b;
        a / b;
        ~a;
        -a;
    }

    function comparisonUses(Cmp a, Cmp b) public pure {
        a < b;
        a > b;
        a == b;
        a != b; //~ ERROR: cannot apply builtin operator
    }
}
