// ported-from: test/libsolidity/syntaxTests/errors/no_mappings.sol
error MyError(mapping(uint => uint)); //~ ERROR: types containing mappings cannot be error parameter types
contract C {
    error MyError2(mapping(uint => uint)); //~ ERROR: types containing mappings cannot be error parameter types
}
