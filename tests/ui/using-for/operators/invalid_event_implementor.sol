// ported-from: test/libsolidity/syntaxTests/operators/userDefined/implementing_operator_with_event.sol

type U is uint256;

contract C {
    event E(U, U);
}

using {C.E as *} for U global; //~ ERROR: expected function name
