// ported-from: test/libsolidity/syntaxTests/types/mapping/error_parameter.sol
error E (mapping (uint => uint)); //~ ERROR: types containing mappings cannot be error parameter types
