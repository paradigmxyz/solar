// ported-from: test/libsolidity/syntaxTests/constantEvaluator/type_reference.sol
// ported-from: test/libsolidity/syntaxTests/constantEvaluator/type_reference_in_contract.sol

int[L] constant L = 6; //~ ERROR: failed to evaluate constant
//~^ ERROR: only constants of value type and byte array type are implemented

contract C {
    int[M] constant M = 6; //~ ERROR: failed to evaluate constant
    //~^ ERROR: only constants of value type and byte array type are implemented
}
